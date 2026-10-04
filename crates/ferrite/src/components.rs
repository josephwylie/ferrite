//! Ferrite's shared primitives: the one way render code opens a text run,
//! lays a plane or a floating surface, draws a status mark or a keycap,
//! builds a control or a menu row, and runs the few loops the terminal
//! grammar allows (the braille and working spinners, the shimmer, the caret
//! blink — theme rule 8). Longbridge owns the control mechanics; `theme.rs`
//! is the only token source.
//!
//! Frozen after the foundation (F3): a package that needs something new
//! builds it privately and asks for a promotion. A bug fix comes with a
//! failing test first. Styles are asserted as data in `tests` below.

use std::ops::Range;
use std::time::Duration;

use gpui::component::button::{Button, ButtonVariants};
use gpui::component::{FocusableExt, Sizable};
use gpui::prelude::*;
use gpui::{
    div, point, px, rgb, rgba, AnyElement, App, BoxShadow, Div, ElementId, FontFeatures,
    HighlightStyle, Hsla, SharedString, Stateful, StyleRefinement, Window,
};

use crate::icons;
use crate::motion;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme;

/// A compact, neutral button. Supply content with its own typography so
/// upstream control sizes and hover foregrounds cannot recolour the label.
pub fn button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .tab_stop(false)
        .focus_ring(false)
        .border_0()
        .focus_visible(control_focus)
        .rounded(px(theme::R_CONTROL))
        .font_family(theme::FONT_UI)
        .cursor_pointer()
}

/// Keyboard focus, the one recipe: a `FOCUS_RING_W` (1px) inset
/// `FOCUS_RING` outline. It survives hover's border/background refinements
/// and stays inside clipped forms without taking any layout space, so a
/// control never swaps a border to say it has focus.
pub fn control_focus(style: StyleRefinement) -> StyleRefinement {
    focus_outline(style, theme::FOCUS_RING)
}

fn focus_outline(style: StyleRefinement, ink: u32) -> StyleRefinement {
    style.shadow(vec![BoxShadow {
        inset: true,
        color: rgb(ink).into(),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(theme::FOCUS_RING_W),
    }])
}

/// `control_focus` on an element whose focus is a state the caller tracks
/// (tab resting on a band chip, an editing field), not the element's own.
pub fn focused<E: Styled>(mut element: E, focused: bool) -> E {
    if focused {
        let ring = control_focus(StyleRefinement::default());
        element.style().refine(&ring);
    }
    element
}

/// One hover blend for a kit `Button` (rule 2.10.2): its ground blends
/// `rest → hover` over 150ms under a key derived from its id, the kit's own
/// hover is held at that ground (`pointer::button_variant`) so it never
/// snaps over the blend, and the press lands at once on `press`. The caller
/// sets no ground of its own after it.
pub fn faded_button(
    id: impl Into<ElementId>,
    rest: Hsla,
    hover: Hsla,
    press: Hsla,
    ink: Hsla,
    cx: &App,
) -> Button {
    let id = id.into();
    let key = crate::pointer::hover_key(&id);
    let ground = motion::hover_blend(&key, rest, hover);
    // The ground is also the caller-layer style: the kit draws a custom
    // variant's rest a fifth toward transparent, and replays this layer in
    // its selected state, so the blended face is exactly what shows.
    // A clear ground paints nothing of its own.
    button(id)
        .custom(crate::pointer::button_variant(ground, ink, press, cx))
        .when(ground.a > 0.0, |button| button.bg(ground))
        .on_hover(motion::hover_listener(key))
}

/// A text tooltip on a faded kit `Button`, in the floating vocabulary
/// (`menu::tooltip`). The kit's own `tooltip` rides an `on_hover` listener,
/// and an element carries one: the blend's. This one is gpui's own.
pub trait Tip {
    fn tip(self, text: impl Into<SharedString>) -> Self;
}

impl Tip for Button {
    fn tip(mut self, text: impl Into<SharedString>) -> Self {
        self.interactivity().tooltip(crate::menu::tooltip(text));
        self
    }
}

/// A form action: nothing at rest, `paint::HOVER` under the pointer
/// (blended), `paint::PRESS` pressed.
pub fn form_button(id: impl Into<ElementId>, cx: &App) -> Button {
    form_button_on(id, rgba(theme::TRANSPARENT).into(), cx)
}

/// `form_button` resting on its own `rest` ground (a filled field-like
/// control): the same blend and press.
pub fn form_button_on(id: impl Into<ElementId>, rest: Hsla, cx: &App) -> Button {
    faded_button(
        id,
        rest,
        theme::paint::HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(theme::TEXT).into(),
        cx,
    )
    .tab_stop(true)
}

/// The completing action: the accent as a fill (`ACCENT_STRONG`) with dark
/// `ON_ACCENT` ink, hovering to `PRIMARY_HOVER` and pressing to
/// `PRIMARY_ACTIVE`; disabled is `SELECTION` with `TEXT_MUTED` ink. Flat,
/// square. On the filled face the focus outline is `TEXT_STRONG`.
pub fn primary_button(id: impl Into<ElementId>, disabled: bool, cx: &App) -> Button {
    use gpui::component::Disableable;
    let face: Hsla = rgb(primary_face(disabled)).into();
    let (hover, press) = if disabled {
        (face, face)
    } else {
        (
            rgb(theme::PRIMARY_HOVER).into(),
            rgb(theme::PRIMARY_ACTIVE).into(),
        )
    };
    faded_button(
        id,
        face,
        hover,
        press,
        rgb(primary_ink(disabled)).into(),
        cx,
    )
    .tab_stop(true)
    .text_color(rgb(primary_ink(disabled)))
    .focus_visible(|style| focus_outline(style, theme::TEXT_STRONG))
    .disabled(disabled)
    .when(disabled, |button| button.cursor_default())
}

fn primary_face(disabled: bool) -> u32 {
    if disabled {
        theme::SELECTION
    } else {
        theme::ACCENT_STRONG
    }
}

fn primary_ink(disabled: bool) -> u32 {
    if disabled {
        theme::TEXT_MUTED
    } else {
        theme::ON_ACCENT
    }
}

/// Forms use the body size so values and actions read at the same scale as
/// their labels. Dense pane chrome continues to use `label`.
pub fn form_label(text: impl Into<SharedString>, ink: u32) -> impl IntoElement {
    div()
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_UI))
        .line_height(gpui::px(theme::LH_UI))
        .text_color(rgb(ink))
        .child(text.into())
}

// ------------------------------------------------------------------- type

/// A line of the grid: Geist Mono (`FONT_UI`) · `FS_UI` 13 on `LH_UI` 20 ·
/// `TEXT`.
pub fn text_ui() -> Div {
    div()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(theme::TEXT))
}

/// Metadata: the same grid line in `TEXT_MUTED` (one size per surface:
/// metadata is ink, not a smaller size).
pub fn text_meta() -> Div {
    div()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_MUTED))
}

/// A group's title inside a surface: UI `FS_SM` `W_LABEL` `TEXT_MUTED`,
/// written as-is (terminal case, no rule), 8px above what it heads.
pub fn section_label(text: impl Into<SharedString>) -> Div {
    text_meta()
        .font_weight(theme::W_LABEL)
        .pb(px(theme::SPACE_2))
        .child(text.into())
}

/// Tabular figures, so a ticking count or a column of numbers never shifts.
pub fn tabular<E: Styled>(mut element: E) -> E {
    element.text_style().font_features =
        Some(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])));
    element
}

// ------------------------------------------------- planes and elevation

/// The old elevation ladder's rungs, kept as names (theme rule 2: square
/// and flat inside). Only a floating surface casts; nothing is lit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elevation {
    /// A Pane on the field: flat.
    Pane,
    /// The focused Pane of a board: flat (its border says focus).
    Lifted,
    /// A control's face: flat.
    Control,
    /// Menus, pickers, the palette, popovers, tooltips, a dragged ghost:
    /// the one float shadow.
    Float,
    /// A modal sheet over the veil: the float shadow (the veil does the
    /// rest).
    Sheet,
}

fn layer(color: u32, y: f32, blur: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        inset: false,
        color: rgba(color).into(),
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
    }
}

/// A rung's shadow: nothing, except a floating surface's one small shadow
/// (`float_shadow`).
pub fn elevation(rung: Elevation) -> Vec<BoxShadow> {
    match rung {
        Elevation::Pane | Elevation::Lifted | Elevation::Control => Vec::new(),
        Elevation::Float | Elevation::Sheet => float_shadow(),
    }
}

/// `float_shadow` with its ink scaled by `k` (0..1): a floating surface's
/// shadow while it fades in (`motion::menu_in`).
pub fn float_shadow_faded(k: f32) -> Vec<BoxShadow> {
    float_shadow()
        .into_iter()
        .map(|layer| BoxShadow {
            color: layer.color.opacity(k.clamp(0.0, 1.0)),
            ..layer
        })
        .collect()
}

/// The one cast shadow (theme rule 2): CSS `0 6px 14px` at 50%, under
/// every floating surface and nothing else.
pub fn float_shadow() -> Vec<BoxShadow> {
    vec![layer(
        theme::SHADOW_FLOAT,
        theme::SHADOW_FLOAT_Y,
        theme::SHADOW_FLOAT_BLUR,
        0.,
    )]
}

/// A floating surface (menu, picker, palette, popover, card): the float
/// ground (`paint::FLOAT`, near-opaque glass on macOS), a 1px `paint::LINE2`
/// edge, square, the float shadow, `FLOAT_PAD` inside, the grid's type. It
/// occludes what it covers and owns its cursor. The caller states its
/// width and position.
pub fn floating_surface() -> Div {
    text_ui()
        .cursor_default()
        .occlude()
        .flex()
        .flex_col()
        .p(px(theme::FLOAT_PAD))
        .rounded(px(theme::R_BLOCK))
        .bg(theme::paint::FLOAT)
        .border_1()
        .border_color(theme::paint::LINE2)
        .shadow(float_shadow())
}

/// The modal veil: covers its parent, takes every press, centres its child.
pub fn veil() -> Div {
    div()
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(theme::VEIL))
}

/// The reading column: at most `READING_MAX_W`, centred in a wide Pane, the
/// full width of a narrow one.
pub fn reading_column(child: impl IntoElement) -> Div {
    div()
        .w_full()
        .max_w(px(theme::READING_MAX_W))
        .mx_auto()
        .min_w_0()
        .child(child)
}

// ------------------------------------------------------------------ marks

/// A status dot, `STATUS_DOT` across. Chain `.size(..)` for another size.
pub fn status_dot(ink: u32) -> Div {
    div()
        .flex_shrink_0()
        .size(px(theme::STATUS_DOT))
        .rounded_full()
        .bg(rgb(ink))
}

/// A hollow status dot (parked): the ring without the fill.
pub fn status_ring(ink: u32) -> Div {
    div()
        .flex_shrink_0()
        .size(px(theme::STATUS_DOT))
        .rounded_full()
        .border_1()
        .border_color(rgb(ink))
}

// ------------------------------------------------------------------ loops
//
// The few loops the terminal grammar allows (theme rule 8). Each rides the
// shared pulse clock (`motion::pulse_phase`: one ~30fps tick for the whole
// window, parked when nothing loops), leasing the view that paints it, and
// holds a static end state under reduced motion that says the same thing.

#[allow(unused_imports)] // until the builders call them
pub use loops::*;

/// The loops (see the section note above): re-exported, so callers name
/// `components::braille_spinner` and friends.
#[allow(dead_code)] // the builders' API: the sidebar, Pane heads, the working line, the Composer
mod loops {
    use super::*;

    /// The braille spinner's frames as characters, `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, for copy and
    /// tests: what `icons::BRAILLE_FRAMES` draws (Geist Mono has no braille, so
    /// they are never text, rule 10).
    pub const BRAILLE_FRAMES: [char; 10] = [
        '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}',
        '\u{2827}', '\u{2807}', '\u{280f}',
    ];

    /// The working spinner's frames as characters, `· ✢ ✳ ✶ ✻ ✽ ✻ ✶ ✳ ✢`: what
    /// `icons::WORKING_FRAMES` draws.
    pub const WORKING_FRAMES: [char; 10] = [
        '\u{b7}', '\u{2722}', '\u{2733}', '\u{2736}', '\u{273b}', '\u{273d}', '\u{273b}',
        '\u{2736}', '\u{2733}', '\u{2722}',
    ];

    /// Which of `frames` a loop shows at `phase` [0, 1) of its turn.
    pub fn frame_at(phase: f32, frames: usize) -> usize {
        ((phase.rem_euclid(1.0) * frames as f32) as usize).min(frames.saturating_sub(1))
    }

    /// The braille spinner (theme rule 8): `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, one frame per
    /// `MOTION_BRAILLE_FRAME_MS`, in `ink` (`RUNNING`), drawn as an SVG in a
    /// `size` glyph box. It stands in place of a *working* Thread's status dot,
    /// in the sidebar and in Pane heads, so a board of them shares one tick.
    /// Under reduced motion it is the still dot (`status_dot(ink)`), centred in
    /// the same box.
    pub fn braille_spinner(ink: u32, size: f32) -> AnyElement {
        Spinner {
            frames: &icons::BRAILLE_FRAMES,
            frame_ms: theme::MOTION_BRAILLE_FRAME_MS,
            still: Still::Dot,
            ink,
            size,
            selector: "braille-spinner",
        }
        .into_any_element()
    }

    /// The working line's spinner (theme rule 8): `· ✢ ✳ ✶ ✻ ✽ ✻ ✶ ✳ ✢`, one
    /// frame per `MOTION_WORKING_FRAME_MS`, in `ink` (the provider's brand
    /// colour: `PROVIDER_CLAUDE`, `PROVIDER_CODEX`), drawn as an SVG in a
    /// `size` glyph box — the focused Pane's gutter mark while a turn runs.
    /// Under reduced motion it holds `✻` (`icons::WORKED`), the mark a finished
    /// turn wears.
    pub fn working_spinner(ink: u32, size: f32) -> AnyElement {
        Spinner {
            frames: &icons::WORKING_FRAMES,
            frame_ms: theme::MOTION_WORKING_FRAME_MS,
            still: Still::Frame(icons::WORKED),
            ink,
            size,
            selector: "working-spinner",
        }
        .into_any_element()
    }

    #[derive(Clone, Copy)]
    enum Still {
        /// The status dot.
        Dot,
        /// One drawn frame.
        Frame(&'static str),
    }

    #[derive(IntoElement)]
    struct Spinner {
        frames: &'static [&'static str],
        frame_ms: u64,
        still: Still,
        ink: u32,
        size: f32,
        selector: &'static str,
    }

    impl RenderOnce for Spinner {
        fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
            let mark = if motion::reduced_motion(cx) {
                match self.still {
                    Still::Dot => status_dot(self.ink).into_any_element(),
                    Still::Frame(path) => icons::icon(path, self.size, self.ink).into_any_element(),
                }
            } else {
                let turn = Duration::from_millis(self.frame_ms * self.frames.len() as u64);
                let phase = motion::pulse_phase(turn, window.current_view(), cx);
                let frame = self.frames[frame_at(phase, self.frames.len())];
                icons::icon(frame, self.size, self.ink).into_any_element()
            };
            let selector = self.selector;
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(self.size))
                .debug_selector(move || selector.into())
                .child(mark)
        }
    }

    /// The colour the shimmer crests at over `base`: `base` lifted toward white
    /// by `SHIMMER_LIFT` (Claude's clay crests near `#ffe1d3`).
    pub fn shimmer_crest(base: u32) -> Hsla {
        motion::mix(rgb(base).into(), rgb(0xffffff).into(), theme::SHIMMER_LIFT)
    }

    /// The per-character colours of the shimmer at `phase` [0, 1): a soft crest
    /// `2 × SHIMMER_HALF_WIDTH` of the run wide, linear on each side, travelling
    /// left to right from half a run before the text to half a run past it (the
    /// prototype's 300% gradient). Characters off the crest carry no highlight
    /// (they take the run's own `base` colour). Byte ranges, sorted, disjoint.
    pub fn shimmer_highlights(
        text: &str,
        base: Hsla,
        crest: Hsla,
        phase: f32,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let count = text.chars().count();
        if count == 0 {
            return Vec::new();
        }
        let centre = -0.5 + 2.0 * phase.rem_euclid(1.0);
        text.char_indices()
            .enumerate()
            .filter_map(|(index, (at, ch))| {
                let x = (index as f32 + 0.5) / count as f32;
                let k = 1.0 - (x - centre).abs() / theme::SHIMMER_HALF_WIDTH;
                (k > 0.0).then(|| {
                    (
                        at..at + ch.len_utf8(),
                        HighlightStyle {
                            color: Some(motion::mix(base, crest, k)),
                            ..Default::default()
                        },
                    )
                })
            })
            .collect()
    }

    /// The working caption's shimmer (theme rule 8): `text` in `base` (the
    /// provider's colour) under a crest of `shimmer_crest(base)` sweeping left
    /// to right every `MOTION_SHIMMER_MS`. One `StyledText`, recoloured per
    /// character, so nothing reflows; one line that truncates at its end. Only
    /// the focused Pane's working line wears it; under reduced motion it is
    /// plain `base`. The caller sets the face, size and line height.
    pub fn shimmer(text: impl Into<SharedString>, base: u32) -> AnyElement {
        Shimmer {
            text: text.into(),
            base,
        }
        .into_any_element()
    }

    #[derive(IntoElement)]
    struct Shimmer {
        text: SharedString,
        base: u32,
    }

    impl RenderOnce for Shimmer {
        fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
            let base: Hsla = rgb(self.base).into();
            let text = if motion::reduced_motion(cx) {
                gpui::StyledText::new(self.text)
            } else {
                let turn = Duration::from_millis(theme::MOTION_SHIMMER_MS);
                let phase = motion::pulse_phase(turn, window.current_view(), cx);
                let highlights =
                    shimmer_highlights(&self.text, base, shimmer_crest(self.base), phase);
                gpui::StyledText::new(self.text).with_highlights(highlights)
            };
            div()
                .min_w_0()
                .truncate()
                .text_color(base)
                .debug_selector(|| "shimmer".into())
                .child(text)
        }
    }

    /// The soft block caret's opacity at `phase` [0, 1) of
    /// `MOTION_CARET_BLINK_MS` (the prototype's keyframes): full to 45%, eased
    /// down to `CARET_BLINK_MIN` by 55%, held to 95%, eased back to full.
    pub fn caret_blink(phase: f32) -> f32 {
        const EASE_IN_OUT: motion::CubicBezier = motion::CubicBezier::new(0.42, 0.0, 0.58, 1.0);
        let low = theme::CARET_BLINK_MIN;
        let p = phase.rem_euclid(1.0);
        if p < 0.45 {
            1.0
        } else if p < 0.55 {
            motion::lerp(1.0, low, EASE_IN_OUT.eval((p - 0.45) / 0.10))
        } else if p < 0.95 {
            low
        } else {
            motion::lerp(low, 1.0, EASE_IN_OUT.eval((p - 0.95) / 0.05))
        }
    }

    /// The focused Composer's caret opacity now, leasing the painting view on
    /// the pulse clock; full and still under reduced motion. An unfocused
    /// Composer draws no blinking caret (its caret is a faint outline, still).
    pub fn caret_opacity(window: &mut Window, cx: &mut App) -> f32 {
        if motion::reduced_motion(cx) {
            return 1.0;
        }
        let turn = Duration::from_millis(theme::MOTION_CARET_BLINK_MS);
        caret_blink(motion::pulse_phase(turn, window.current_view(), cx))
    }
}

/// Ferrite's mark as a flat watermark in its `body` ink: nothing is lit or
/// embossed (theme rule 2). It takes no pointer.
pub fn embossed_mark(size: f32, body: u32) -> Div {
    div().relative().flex_shrink_0().size(px(size)).child(
        gpui::svg()
            .path(icons::FERRITE_MONO)
            .absolute()
            .left_0()
            .top_0()
            .size(px(size))
            .text_color(rgb(body)),
    )
}

/// The one keycap: `KBD_H`, at least square, flat and square on
/// `paint::BAND2`, the grid's type in `TEXT_2`, centred.
pub fn kbd(key: impl Into<SharedString>) -> Div {
    kbd_face().child(key.into())
}

fn kbd_face() -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(theme::KBD_H))
        .min_w(px(theme::KBD_H))
        .px(px(theme::KBD_PAD_X))
        .rounded(px(theme::R_CHIP))
        .bg(theme::paint::BAND2)
        .font_family(theme::FONT_CODE)
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_2))
}

/// A modifier's glyph as a key combination spells it: `cmd` ⌘, `shift` ⇧,
/// `alt` ⌥, `ctrl` ⌃. `None` for a key that is its own word.
#[cfg(test)]
pub fn key_glyph(part: &str) -> Option<char> {
    match part {
        "cmd" => Some('\u{2318}'),
        "shift" => Some('\u{21e7}'),
        "alt" => Some('\u{2325}'),
        "ctrl" => Some('\u{2303}'),
        _ => None,
    }
}

/// A key table's combination as `key_combo` draws it, in characters:
/// `cmd-shift-N` → `⌘⇧N`, `cmd shift N` → `⌘ ⇧ N`. What a reader sees and
/// what a test compares.
#[cfg(test)]
pub fn key_glyphs(keys: &str) -> String {
    let glyph = |part: &str| key_glyph(part).map_or_else(|| part.to_string(), String::from);
    keys.split(' ')
        .map(|word| word.split('-').map(glyph).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A key combination as it is drawn, from a key table's spelling, in the
/// code face (keys are machine text, rule 6). Every modifier is a glyph in
/// a `KEY_GLYPH` box: `⌘` (`command.svg`), `⌥` (`option.svg`) and `⌃`
/// (`control.svg`) are in neither face, and `⇧` is Geist Mono's own
/// (`CHROME_GLYPHS`); every other part stays its own word. Parts joined by
/// `-` sit tight, as a menu shortcut or a tooltip reads (`cmd-F` → `⌘F`);
/// parts joined by spaces keep one code space apart. The one place a
/// modifier glyph is drawn.
pub fn key_combo(keys: &str, ink: u32) -> Div {
    let spaced = keys.contains(' ');
    let gap = if spaced {
        theme::FS_SM * theme::CODE_ADVANCE
    } else {
        0.
    };
    let glyph_box = || {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .w(px(theme::KEY_GLYPH))
    };
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .font_family(theme::FONT_CODE)
        .gap(px(gap))
        .text_color(rgb(ink))
        .children(keys.split([' ', '-']).map(|part| {
            let svg = match part {
                "cmd" => Some((icons::COMMAND, "command-key")),
                "alt" => Some((icons::OPTION, "option-key")),
                "ctrl" => Some((icons::CONTROL, "control-key")),
                _ => None,
            };
            match (svg, part) {
                (Some((path, selector)), _) => glyph_box()
                    .debug_selector(move || selector.into())
                    .child(icons::icon(path, theme::KEY_GLYPH, ink))
                    .into_any_element(),
                (None, "shift") => glyph_box()
                    .debug_selector(|| "shift-key".into())
                    .child("\u{21e7}")
                    .into_any_element(),
                (None, key) => SharedString::from(key.to_string()).into_any_element(),
            }
        }))
}

/// The prompt mark `❯`, drawn (neither face has the glyph): `prompt.svg` in a
/// `GLYPH_BOX`. The transcript prompt and the Composer share it; `ink` is
/// `ACCENT` where it marks the live input, `TEXT_MUTED` where it does not.
pub fn prompt_mark(ink: u32) -> AnyElement {
    glyph_box(icons::icon(icons::PROMPT, theme::GLYPH_BOX, ink)).into_any_element()
}

/// A `GLYPH_BOX` square that centres its mark.
pub fn glyph_box(mark: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::GLYPH_BOX))
        .child(mark)
}

/// The row gutter: `GUTTER_W` (C1) wide, its mark centred on the first line
/// box (`first_line_h` high), so text starts at C1 on every row.
pub fn gutter(mark: impl IntoElement, first_line_h: f32) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(theme::GUTTER_W))
        .h(px(first_line_h))
        .child(glyph_box(mark))
}

/// A floating surface's empty list: one line of guidance, centred, the
/// title in `TEXT_2` and an optional hint in `TEXT_MUTED` beneath it. It is
/// never a Pane body: an empty Thread or draft shows nothing, and its
/// Composer's placeholder says what to do (rule 2.11.4).
pub fn empty_state(title: impl Into<SharedString>, hint: Option<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(theme::SPACE_1))
        .size_full()
        .child(text_ui().text_color(rgb(theme::TEXT_2)).child(title.into()))
        .children(hint.map(|hint| text_meta().child(hint)))
}

// --------------------------------------------------------------- controls

/// The chord an action is bound to with no key context, as a menu shortcut
/// spells it (`cmd-D`, the last key upper-cased; `esc`, `↵`): `None` when
/// nothing binds it, so a tooltip never names a key that would not act.
pub fn bound_chord(action: &str) -> Option<String> {
    let (keys, _, _) = crate::keymap::bindings(crate::keymap::PLATFORM)
        .into_iter()
        .find(|(_, bound, context)| *bound == action && context.is_none())?;
    let mut parts: Vec<String> = keys.split('-').map(str::to_string).collect();
    if let Some(key) = parts.last_mut() {
        *key = key_word(key);
    }
    Some(parts.join("-"))
}

/// A key table's key as a combination spells it: a letter upper-cased,
/// `escape` as `esc` and `enter` as `↵`, the words a keycap prints.
fn key_word(key: &str) -> String {
    match key {
        "escape" => "esc".into(),
        "enter" => "\u{21b5}".into(),
        key => key.to_uppercase(),
    }
}

/// An icon-only control: `ICON_BUTTON` square, the glyph at
/// `ICON_BUTTON_GLYPH` in `TEXT_MUTED`, brightening to `TEXT` over
/// `paint::HOVER` under the pointer, a tooltip naming what it does.
pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: &'static str,
    tooltip: &'static str,
    cx: &App,
) -> Button {
    faded_button(
        id,
        rgba(theme::TRANSPARENT).into(),
        theme::paint::HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(theme::TEXT_MUTED).into(),
        cx,
    )
    .group(ICON_BUTTON_GROUP)
    .size(px(theme::ICON_BUTTON))
    .tip(tooltip)
    .accessibility_label(tooltip)
    .child(
        icons::icon(glyph, theme::ICON_BUTTON_GLYPH, theme::TEXT_MUTED)
            .group_hover(ICON_BUTTON_GROUP, |style| {
                style.text_color(rgb(theme::TEXT))
            }),
    )
}

/// An `svg()` paints from its own style, never an ambient text colour, so
/// the button's hover reaches its glyph through a named group. Every icon
/// button shares the name: `group_hover` resolves to the nearest one.
const ICON_BUTTON_GROUP: &str = "icon-button";

/// A quiet text control: `CONTROL_H`, the grid's type, `W_BODY` `TEXT_2`;
/// hover `paint::HOVER`, press `paint::PRESS`. A button is read like any
/// row, so it never takes the heading weight.
pub fn ghost_button(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    faded_button(
        id,
        rgba(theme::TRANSPARENT).into(),
        theme::paint::HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(theme::TEXT_2).into(),
        cx,
    )
    .h(px(theme::CONTROL_H))
    .px(px(theme::CONTROL_PAD_X))
    .child(
        text_ui()
            .font_weight(theme::W_BODY)
            .text_color(rgb(theme::TEXT_2))
            .child(label.into()),
    )
}

/// A text-only control with no ground at all (`Back to Main`): `CONTROL_H`,
/// UI `FS_UI` `W_BODY` `TEXT_MUTED`, brightening to `TEXT` under the
/// pointer. For a way out that must stay quieter than the line it ends.
pub fn quiet_button(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    let none: gpui::Hsla = rgba(theme::TRANSPARENT).into();
    button(id)
        .custom(crate::pointer::button_variant(
            none,
            rgb(theme::TEXT_MUTED).into(),
            none,
            cx,
        ))
        .group(QUIET_BUTTON_GROUP)
        .h(px(theme::CONTROL_H))
        .px(px(theme::CONTROL_PAD_X))
        .child(
            text_ui()
                .font_weight(theme::W_BODY)
                .text_color(rgb(theme::TEXT_MUTED))
                .group_hover(QUIET_BUTTON_GROUP, |style| {
                    style.text_color(rgb(theme::TEXT))
                })
                .child(label.into()),
        )
}

/// `quiet_button`'s hover reaches its label through this group.
const QUIET_BUTTON_GROUP: &str = "quiet-button";

// ------------------------------------------------------------------ menus

/// One menu row's content.
#[derive(Clone, Debug, Default)]
pub struct MenuItem {
    pub label: SharedString,
    /// Fuzzy-match runs in `label`, drawn in the accent (never a weight).
    pub matched: Vec<Range<usize>>,
    /// An aligned name column's width, when the rows share one.
    pub label_w: Option<f32>,
    /// A 12px leading mark and its ink.
    pub leading: Option<(&'static str, u32)>,
    /// A trailing detail: Ferrite's description of the row in Geist
    /// `FS_UI` (`TEXT_MUTED`, `TEXT_2` on the cursor row), or — when `mono`
    /// — machine text such as an `@` path, in Geist Mono `FS_SM`
    /// `TEXT_MUTED`, cut at its head so the useful tail survives.
    pub detail: Option<SharedString>,
    /// `detail` is machine text (rule 2.1.1): drawn in the code face.
    pub mono: bool,
    /// The key that does the same thing, faint at the right edge.
    pub shortcut: Option<SharedString>,
    pub checked: bool,
    /// Arms before it runs: at rest it reads like any row; armed, its label
    /// turns `BLOCKED` and asks for the second press.
    pub destructive: bool,
    pub disabled: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            ..Default::default()
        }
    }
    pub fn matched(mut self, ranges: Vec<Range<usize>>) -> Self {
        self.matched = ranges;
        self
    }
    pub fn label_w(mut self, width: f32) -> Self {
        self.label_w = Some(width);
        self
    }
    pub fn leading(mut self, path: &'static str, ink: u32) -> Self {
        self.leading = Some((path, ink));
        self
    }
    pub fn detail(mut self, text: impl Into<SharedString>) -> Self {
        self.detail = Some(text.into());
        self
    }
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }
    /// An empty shortcut is no shortcut.
    pub fn shortcut(mut self, key: impl Into<SharedString>) -> Self {
        let key = key.into();
        self.shortcut = (!key.is_empty()).then_some(key);
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A menu row's inks in one state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowInks {
    pub label: u32,
    pub detail: u32,
    pub shortcut: u32,
    /// `0xRRGGBBAA`, when the row carries a ground.
    pub ground: Option<u32>,
}

/// The row state table, as data: rest, cursor, disabled, armed. A
/// destructive row reads like any other until it arms; armed, it holds the
/// cursor's `paint::SELECTION` ground and its label alone turns `BLOCKED` —
/// colour on the word, never a wash (rule 2.2.4).
pub fn row_inks(item: &MenuItem, cursor: bool, armed: bool) -> RowInks {
    if item.disabled {
        return RowInks {
            label: theme::TEXT_MUTED,
            detail: theme::TEXT_MUTED,
            shortcut: theme::TEXT_MUTED,
            ground: None,
        };
    }
    if armed {
        return RowInks {
            label: theme::BLOCKED,
            detail: theme::TEXT_2,
            shortcut: theme::TEXT_MUTED,
            ground: Some(theme::paint::SELECTION.rgba()),
        };
    }
    let label = if cursor {
        theme::TEXT_STRONG
    } else {
        theme::TEXT
    };
    RowInks {
        label,
        detail: if cursor {
            theme::TEXT_2
        } else {
            theme::TEXT_MUTED
        },
        shortcut: theme::TEXT_MUTED,
        ground: cursor.then_some(theme::paint::SELECTION.rgba()),
    }
}

/// Fuzzy-match runs as highlights: the accent colour, weight unchanged.
/// A disabled row paints none.
pub fn match_highlights(
    ranges: &[Range<usize>],
    disabled: bool,
) -> Vec<(Range<usize>, HighlightStyle)> {
    if disabled {
        return Vec::new();
    }
    ranges
        .iter()
        .map(|range| {
            (
                range.clone(),
                HighlightStyle {
                    color: Some(rgb(theme::ACCENT).into()),
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// An aligned name column (the `/command` names, code text) for `chars`
/// characters, clamped between `MENU_NAME_MIN_W` and `MENU_NAME_MAX_W`.
/// The row sets names in such a column in the code face.
pub fn code_column_w(chars: usize) -> f32 {
    (chars as f32 * theme::CODE_CELL).clamp(theme::MENU_NAME_MIN_W, theme::MENU_NAME_MAX_W)
}

/// What an armed destructive row adds to its label.
const ARMED_SEAM: &str = " · ";
const ARMED_ASK: &str = "press again";

/// A menu row's content with no id and no pointer role, for kit hosts
/// (`PopupMenuItem::element`) that own the row's interaction.
pub fn menu_row_content(item: &MenuItem, cursor: bool, armed: bool) -> Div {
    let inks = row_inks(item, cursor, armed);
    // Armed, the label asks for the second press in the one line grammar:
    // `Delete thread · press again`, the `·` in structure ink.
    let (label, highlights): (SharedString, _) = if armed {
        let at = item.label.len();
        let text = format!("{}{ARMED_SEAM}{ARMED_ASK}", item.label);
        let seam = HighlightStyle {
            color: Some(rgb(theme::TEXT_FAINT).into()),
            ..Default::default()
        };
        (text.into(), vec![(at..at + ARMED_SEAM.len(), seam)])
    } else {
        (
            item.label.clone(),
            match_highlights(&item.matched, item.disabled),
        )
    };
    text_ui()
        .flex()
        .items_center()
        .gap(px(theme::MENU_ROW_GAP))
        .min_w_0()
        .h(px(theme::MENU_ROW_H))
        .px(px(theme::MENU_ROW_PAD_X))
        .rounded(px(theme::R_MENU_ROW))
        .when_some(inks.ground, |row, ground| row.bg(rgba(ground)))
        .text_color(rgb(inks.label))
        .when_some(item.leading, |row, (path, ink)| {
            row.child(icons::icon(path, theme::ROW_ICON, ink))
        })
        .child(
            div()
                .min_w_0()
                .truncate()
                .when_some(item.label_w, |label, width| {
                    // An aligned column holds names that are code.
                    label
                        .w(px(width))
                        .flex_shrink_0()
                        .font_family(theme::FONT_CODE)
                })
                .child(gpui::StyledText::new(label).with_highlights(highlights)),
        )
        .when_some(item.detail.clone(), |row, detail| {
            let cell = div().flex_1().min_w_0().truncate();
            let cell = if item.mono {
                cell.font_family(theme::FONT_CODE)
                    .text_size(px(theme::FS_SM))
                    .text_color(rgb(theme::TEXT_MUTED))
                    .text_ellipsis_start()
            } else {
                cell.font_family(theme::FONT_UI)
                    .text_size(px(theme::FS_UI))
                    .text_color(rgb(inks.detail))
            };
            row.child(cell.child(detail))
        })
        .when(item.detail.is_none(), |row| row.child(div().flex_1()))
        .when_some(item.shortcut.clone(), |row, key| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(theme::FS_SM))
                    .text_color(rgb(inks.shortcut))
                    .child(key),
            )
        })
        .when(item.checked, |row| {
            row.child(icons::icon(icons::CHECK, theme::ROW_ICON, theme::ACCENT))
        })
}

/// A menu row. The only place a menu row takes its pointer role: the raised
/// hover and press faces, or the carried face on the cursor row; an armed or
/// disabled row takes none. Callers add selectors and handlers only.
pub fn menu_row(
    id: impl Into<ElementId>,
    item: &MenuItem,
    cursor: bool,
    armed: bool,
) -> Stateful<Div> {
    let id = id.into();
    let key = crate::pointer::hover_key(&id);
    let row = menu_row_content(item, cursor, armed).id(id);
    if item.disabled || armed {
        row
    } else if cursor {
        row.hover_carried(key).press_raised()
    } else {
        row.hover_raised(key).press_raised()
    }
}

/// A menu section title: UI `FS_SM` `W_LABEL` `TEXT_MUTED`, an optional
/// leading mark and an optional note after it. Its mark and title share the
/// rows' leading edge. A section that follows rows is set apart from them
/// by `menu_separator` (space) or `.mt(MENU_GROUP_GAP)`, never a rule.
pub fn menu_section(
    title: impl Into<SharedString>,
    leading: Option<(&'static str, u32)>,
    note: Option<SharedString>,
) -> Div {
    // The title's line box sits on the row's foot; the mark is centred on
    // that line box, not on the row, so it rides the text's cap band instead
    // of hanging below its baseline.
    text_meta()
        .flex()
        .items_end()
        .h(px(theme::MENU_SECTION_H))
        .px(px(theme::MENU_ROW_PAD_X))
        .pb(px(theme::SPACE_1))
        .cursor_default()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_2))
                .h(px(theme::LH_META))
                .min_w_0()
                .when_some(leading, |line, (path, ink)| {
                    line.child(icons::icon(path, theme::MENU_SECTION_ICON, ink))
                })
                .child(div().font_weight(theme::W_LABEL).child(title.into()))
                .children(note),
        )
}

/// Reports `element`'s laid-out bounds to `record` in prepaint, through an
/// absolute canvas pinned to all four edges of its padding box, so padding
/// never offsets it. A border is outside that box: an edged caller adds it
/// back. What a summoned surface measures its trigger and limits by.
pub fn on_bounds<E: ParentElement>(
    element: E,
    record: impl FnOnce(gpui::Bounds<gpui::Pixels>, &mut Window, &mut App) + 'static,
) -> E {
    element.child(
        gpui::canvas(
            move |bounds, window, cx| record(bounds, window, cx),
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0(),
    )
}

/// The one separator inside a floating surface: `MENU_GROUP_GAP` of space.
/// Grouping is space, not a line; the surface's own hairline edge is the
/// only rule a menu draws.
pub fn menu_separator() -> Div {
    div().flex_shrink_0().h(px(theme::MENU_GROUP_GAP))
}

/// An inert status line (loading, empty, error) in a menu.
pub fn menu_note(text: impl Into<SharedString>) -> Div {
    text_meta()
        .flex()
        .items_center()
        .h(px(theme::MENU_ROW_H))
        .px(px(theme::MENU_ROW_PAD_X))
        .cursor_default()
        .child(text.into())
}

/// The same menu is opened by a chip or a slash command. PopupMenu owns
/// keyboard navigation, focus, scrolling and dismissal; every row draws in
/// the one menu grammar (`menu_row_content`, `menu_section`, `menu_note`).
#[derive(Clone, Default)]
pub struct Choice {
    pub label: SharedString,
    /// A section's mark, or a row's own when the menu has no sections.
    pub icon: Option<(&'static str, u32)>,
    /// The accent check: the standing choice.
    pub checked: bool,
    pub disabled: bool,
    /// A section title: inert, skipped by the arrows, carrying `icon`.
    pub section: bool,
    /// A muted mono tag after the label (a section tag, a path).
    pub detail: Option<SharedString>,
    /// An inert explanatory line (why the menu is short or locked).
    pub note: bool,
}

impl Choice {
    /// The row's content in the menu grammar.
    fn item(&self, marked: bool) -> MenuItem {
        let mut item = MenuItem::new(self.label.clone())
            .checked(self.checked)
            .disabled(self.disabled);
        if let Some(detail) = &self.detail {
            item = item.detail(detail.clone());
        }
        if let (true, Some((path, ink))) = (marked, self.icon) {
            item = item.leading(path, ink);
        }
        item
    }
}

type OpenChanged = std::rc::Rc<dyn Fn(bool, &mut gpui::Window, &mut gpui::App)>;
type Picked = std::rc::Rc<dyn Fn(usize, &mut gpui::Window, &mut gpui::App)>;

#[derive(IntoElement)]
pub struct ChoiceMenu {
    pub id: SharedString,
    pub trigger: Button,
    /// Which corner of the menu meets the trigger: `BottomLeft` for a
    /// control at the left of its row, `BottomRight` at the right, so the
    /// menu opens over its own Pane rather than across the next one.
    pub anchor: gpui::Anchor,
    pub choices: Vec<Choice>,
    pub open: bool,
    pub return_focus: gpui::FocusHandle,
    pub on_open: OpenChanged,
    pub on_pick: Picked,
    /// Where the menu may rest (`FloatPlace`): its foot `FLOAT_OFFSET`
    /// above the Composer's edge, its right edge inside the Pane's. `None`
    /// hangs it off the trigger alone.
    pub place: Option<FloatPlace>,
}

#[derive(Default)]
struct ChoiceMenuState {
    menu: Option<gpui::Entity<gpui::component::menu::PopupMenu>>,
    steps: usize,
    initialized: bool,
    /// How far the menu is lifted and pulled left of where the kit hangs
    /// it, and whether that has been measured yet (unmeasured, it is
    /// drawn at zero opacity for its one frame).
    lift: f32,
    shift: f32,
    placed: bool,
}

/// The limits a summoned surface rests within (rule 2.4.3): `floor` is the
/// Composer's top edge, which a surface opening upward rests `FLOAT_OFFSET`
/// above so the Composer's edge stays whole; `limit_right` is the Pane's
/// inner edge (`PANE_PAD_X` in from its right), which it never crosses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatPlace {
    pub floor: f32,
    pub limit_right: f32,
}

impl FloatPlace {
    /// A card's anchor point, from its trigger's bounds and never the
    /// pointer: opening up, its foot rests `FLOAT_OFFSET` above the floor;
    /// opening down, its head `FLOAT_OFFSET` under the trigger. It hangs by
    /// its right corner at the trigger's right edge, pulled left to stay
    /// inside the Pane.
    pub fn card_corner(
        &self,
        trigger: gpui::Bounds<gpui::Pixels>,
        up: bool,
    ) -> gpui::Point<gpui::Pixels> {
        let x = f32::from(trigger.right()).min(self.limit_right);
        let y = if up {
            self.floor - theme::FLOAT_OFFSET
        } else {
            f32::from(trigger.bottom()) + theme::FLOAT_OFFSET
        };
        point(px(x), px(y))
    }

    /// How far a surface laid out at `natural` must lift (up is positive)
    /// and shift left to rest on its floor inside the Pane.
    fn offsets(&self, natural: gpui::Bounds<gpui::Pixels>) -> (f32, f32) {
        let lift = f32::from(natural.bottom()) - (self.floor - theme::FLOAT_OFFSET);
        let shift = (f32::from(natural.right()) - self.limit_right).max(0.);
        (lift, shift)
    }
}

impl gpui::RenderOnce for ChoiceMenu {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        use gpui::component::{
            menu::{PopupMenu, PopupMenuItem},
            popover::Popover,
        };
        use gpui::Focusable as _;
        let retained =
            window.use_keyed_state(self.id.clone(), cx, |_, _| ChoiceMenuState::default());
        if !self.open {
            retained.update(cx, |state, _| {
                state.menu = None;
                state.initialized = false;
                state.placed = false;
                state.lift = 0.;
                state.shift = 0.;
            });
        } else if retained.read(cx).menu.is_none() {
            let steps = cursor_steps(&self.choices);
            let pick = self.on_pick.clone();
            // A mark rides its section title; only a menu without sections
            // marks its rows.
            let marked = !self.choices.iter().any(|choice| choice.section);
            let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
                menu = menu
                    .action_context(self.return_focus)
                    .min_w(px(theme::CHOICE_MENU_MIN_W))
                    .max_w(px(theme::CHOICE_MENU_MAX_W))
                    .max_h(px(theme::MENU_MAX_H))
                    .scrollable(true);
                for (index, choice) in self.choices.into_iter().enumerate() {
                    // Sections and notes are disabled element items: the kit
                    // skips them on the arrows, so `steps` still counts only
                    // live rows.
                    if choice.section {
                        let (title, mark) = (choice.label.clone(), choice.icon);
                        // A section after rows stands a group gap off them.
                        let after = index > 0;
                        menu = menu.item(
                            PopupMenuItem::element(move |_, _| {
                                kit_row(
                                    menu_section(title.clone(), mark, None)
                                        .when(after, |section| {
                                            section.mt(px(theme::MENU_GROUP_GAP))
                                        }),
                                )
                            })
                            .disabled(true),
                        );
                        continue;
                    }
                    if choice.note {
                        let text = choice.label.clone();
                        menu = menu.item(
                            PopupMenuItem::element(move |_, _| kit_row(menu_note(text.clone())))
                                .disabled(true),
                        );
                        continue;
                    }
                    // Our accent check draws inside the row, so the kit's own
                    // `.checked()` is never set and its check never doubles.
                    let item = choice.item(marked);
                    let picked = pick.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, _| {
                            // The kit item does not shrink its content, so
                            // the row holds the menu's widest itself and a
                            // long detail truncates inside it.
                            kit_row(menu_row_content(&item, false, false))
                                .max_w(px(theme::CHOICE_MENU_MAX_W - 2. * theme::FLOAT_PAD))
                                .debug_selector(move || format!("choice-row-{index}"))
                        })
                        .disabled(choice.disabled)
                        .on_click(move |_, window, cx| picked(index, window, cx)),
                    );
                }
                menu
            });
            let on_open = self.on_open.clone();
            window
                .subscribe(&menu, cx, move |_, _: &gpui::DismissEvent, window, cx| {
                    on_open(false, window, cx);
                })
                .detach();
            retained.update(cx, |state, _| {
                state.menu = Some(menu);
                state.steps = steps;
            });
        }
        let (menu, lift, shift, placed) = {
            let state = retained.read(cx);
            (state.menu.clone(), state.lift, state.shift, state.placed)
        };
        let place = self.place;
        let on_open = self.on_open;
        let mut popover = Popover::new(SharedString::from(format!("choice:{}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .anchor(self.anchor)
            .trigger(self.trigger)
            .open(self.open)
            .on_open_change(move |open, window, cx| on_open(*open, window, cx));
        if place.is_some() {
            // The kit hangs the menu off the chip; the measured offsets move
            // it onto the Composer's edge and inside the Pane.
            popover = popover
                .bottom(px(lift))
                .left(px(-shift))
                .when(!placed, |popover| popover.opacity(0.));
        }
        if let Some(menu) = menu {
            popover = popover
                .track_focus(&menu.focus_handle(cx))
                .content(move |_, _, _| {
                    let retained = retained.clone();
                    let menu = menu.clone();
                    // The one floating recipe. The kit menu's own inset
                    // (`p_1`, 4px) is the `FLOAT_PAD`, so the surface adds
                    // none; its hairline ring and radius are clipped away
                    // at its own edge, so the only edge is the surface's.
                    let surface = floating_surface()
                        .debug_selector(|| "choice-menu".into())
                        .p(px(0.))
                        .child(div().overflow_hidden().child(menu.clone()));
                    on_bounds(surface, move |bounds, window, cx| {
                        if let Some(place) = place {
                            // Inside the surface's 1px edge: add it back.
                            let bounds = bounds.dilate(px(1.));
                            let moved = retained.update(cx, |state, _| {
                                // Undo the offsets in force to find where
                                // the kit laid it, then solve from there.
                                let natural = gpui::Bounds {
                                    origin: point(
                                        bounds.origin.x + px(state.shift),
                                        bounds.origin.y + px(state.lift),
                                    ),
                                    size: bounds.size,
                                };
                                let (lift, shift) = place.offsets(natural);
                                let moved = (lift - state.lift).abs() > 0.5
                                    || (shift - state.shift).abs() > 0.5
                                    || !state.placed;
                                state.placed = true;
                                if moved {
                                    state.lift = lift;
                                    state.shift = shift;
                                }
                                moved
                            });
                            if moved {
                                window.refresh();
                            }
                        }
                        let steps = retained.update(cx, |state, _| {
                            if state.initialized {
                                return None;
                            }
                            state.initialized = true;
                            Some(state.steps)
                        });
                        if let Some(steps) = steps {
                            menu.focus_handle(cx).focus(window, cx);
                            for _ in 0..steps {
                                window
                                    .dispatch_action(Box::new(gpui::base::actions::SelectDown), cx);
                            }
                        }
                    })
                });
        }
        popover
    }
}

/// How many `SelectDown`s land the kit's cursor on the standing choice (or
/// the first live row). Every choice is one kit item; the first press
/// selects item 0 whatever it is, each later one the next live item.
fn cursor_steps(choices: &[Choice]) -> usize {
    let live = |choice: &Choice| !choice.section && !choice.note && !choice.disabled;
    let target = choices
        .iter()
        .position(|choice| live(choice) && choice.checked)
        .or_else(|| choices.iter().position(live))
        .unwrap_or(0);
    let lives = choices[..=target.min(choices.len().saturating_sub(1))]
        .iter()
        .filter(|choice| live(choice))
        .count();
    let dead_first = choices.first().is_some_and(|choice| !live(choice));
    lives.max(1) + usize::from(dead_first && lives > 0)
}

/// A menu grammar row inside a kit `PopupMenuItem`: the kit item already
/// insets its content by the row's own inline padding, so the row takes it
/// back and spans the item edge to edge. The kit draws the hover and cursor
/// face (`tokens.accent` = `FILL`) on the item itself.
pub fn kit_row(row: Div) -> Div {
    row.flex_1().mx(px(-theme::MENU_ROW_PAD_X)).py(px(0.))
}

/// The scrollbar. gpui paints none of its own, so the toolkit's draws it:
/// an 8px thumb in a 16px gutter that lightens under the pointer, drags,
/// and fades out two seconds after the scroll stops — and nothing at all
/// when the content fits, because an always-on track would be a line, and
/// Soft draws no lines. The colours are `theme::init_components`' own
/// `scrollbar_thumb` tokens, so this stays in Ferrite's palette.
///
/// [`gpui::base::ScrollbarMode::Hover`] is the mode, not the toolkit's default
/// `Scrolling`: under `Scrolling` the bar answers the pointer *only* while
/// it happens to be visible, so once it has faded the gutter is dead and
/// the thumb can never be grabbed — the wheel is the only way to move.
/// Hover keeps the same fade, and brings the thumb back when the pointer
/// enters the gutter, which is the only moment anyone wants to grab it.
///
/// Hang it as a *sibling* of the scrolling element inside a shared
/// `relative()` parent, never as a child, or it scrolls away with the
/// content. The `id` must be unique per scroll area: the toolkit keys the
/// bar's hover, drag and fade state off it, and one helper here means the
/// caller location cannot do that keying for us.
pub fn scrollbar(
    id: impl Into<ElementId>,
    scroll: &(impl gpui::base::ScrollbarHandle + Clone),
) -> gpui::Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(
            crate::scrollbar::Scrollbar::vertical(scroll)
                .id(id)
                .scrollbar_show(gpui::component::scroll::ScrollbarMode::Hover),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Fill, Hsla};

    fn solid(value: u32) -> Hsla {
        rgb(value).into()
    }

    /// Theme rule 2: the one cast shadow, down and small, never inset.
    #[test]
    fn the_float_shadow_is_one_small_cast() {
        let layers = float_shadow();
        assert_eq!(layers.len(), 1);
        let shadow = &layers[0];
        assert!(!shadow.inset);
        assert_eq!(shadow.color, rgba(theme::SHADOW_FLOAT).into());
        assert_eq!(shadow.offset.y, px(theme::SHADOW_FLOAT_Y));
        assert_eq!(shadow.blur_radius, px(theme::SHADOW_FLOAT_BLUR));
        assert_eq!(shadow.spread_radius, px(0.));
    }

    /// Theme rule 2: square and flat inside. Only a float (and a sheet,
    /// which floats) casts; no rung, keycap, well or knob is lit.
    #[test]
    fn only_a_float_casts_and_nothing_is_lit() {
        use Elevation::*;
        for rung in [Pane, Lifted, Control] {
            assert!(elevation(rung).is_empty(), "{rung:?} is flat");
        }
        for rung in [Float, Sheet] {
            assert_eq!(elevation(rung), float_shadow(), "{rung:?} floats");
        }
        assert!(float_shadow_faded(0.5)[0].color.a < float_shadow()[0].color.a);
    }

    #[test]
    fn a_floating_surface_wears_the_float_ground_the_strong_line_and_the_shadow() {
        let mut surface = floating_surface();
        let style = surface.style();
        assert_eq!(style.background, Some(Fill::from(theme::paint::FLOAT)));
        assert_eq!(style.border_color, Some(theme::paint::LINE2.into()));
        assert_eq!(style.corner_radii.top_left, Some(px(0.).into()));
        assert_eq!(style.box_shadow, Some(float_shadow()));
        assert_eq!(style.text.font_family, Some(theme::FONT_UI.into()));
    }

    #[test]
    fn type_openers_pair_each_size_with_its_line_box() {
        for (mut run, face, size, line, ink) in [
            (
                text_ui(),
                theme::FONT_UI,
                theme::FS_UI,
                theme::LH_UI,
                theme::TEXT,
            ),
            (
                text_meta(),
                theme::FONT_UI,
                theme::FS_SM,
                theme::LH_META,
                theme::TEXT_MUTED,
            ),
        ] {
            let text = &run.style().text;
            assert_eq!(text.font_family, Some(face.into()));
            assert_eq!(text.font_size, Some(px(size).into()));
            assert_eq!(text.line_height, Some(px(line).into()));
            assert_eq!(text.color, Some(solid(ink)));
        }
    }

    #[test]
    fn keyboard_focus_is_the_focus_ring_ink_and_primary_is_steel() {
        let focus = control_focus(StyleRefinement::default());
        let shadow = &focus.box_shadow.clone().expect("an inset outline")[0];
        assert!(shadow.inset);
        assert_eq!(shadow.color, solid(theme::FOCUS_RING));
        assert_eq!(shadow.spread_radius, px(theme::FOCUS_RING_W));
        assert_eq!(theme::FOCUS_RING_W, 1.);
        let mut ringed = focused(div(), true);
        assert_eq!(ringed.style().box_shadow, focus.box_shadow);
        let mut plain = focused(div(), false);
        assert_eq!(plain.style().box_shadow, None);
        assert_eq!(primary_face(false), theme::ACCENT_STRONG);
        assert_eq!(primary_ink(false), theme::ON_ACCENT);
        assert_eq!(primary_face(true), theme::FILL);
        assert_eq!(primary_ink(true), theme::TEXT_MUTED);
    }

    #[test]
    fn marks_and_keycaps_hold_their_boxes() {
        let mut dot = status_dot(theme::RUNNING);
        assert_eq!(dot.style().size.width, Some(px(theme::STATUS_DOT).into()));
        assert_eq!(
            dot.style().background,
            Some(Fill::from(rgb(theme::RUNNING)))
        );
        let mut ring = status_ring(theme::TEXT_FAINT);
        assert_eq!(ring.style().background, None);
        assert_eq!(ring.style().border_color, Some(solid(theme::TEXT_FAINT)));
        let mut key = kbd("y");
        assert_eq!(key.style().size.height, Some(px(theme::KBD_H).into()));
        assert_eq!(
            key.style().background,
            Some(Fill::from(theme::paint::BAND2))
        );
        assert_eq!(key.style().box_shadow, None, "a keycap is flat");
        let mut gutter = gutter(div(), theme::LH_UI);
        assert_eq!(gutter.style().size.width, Some(px(theme::GUTTER_W).into()));
    }

    #[test]
    fn menu_rows_follow_the_state_table() {
        let rest = MenuItem::new("Rename");
        let fill = theme::paint::SELECTION.rgba();
        assert_eq!(
            row_inks(&rest, false, false),
            RowInks {
                label: theme::TEXT,
                detail: theme::TEXT_MUTED,
                shortcut: theme::TEXT_MUTED,
                ground: None
            }
        );
        let cursor = row_inks(&rest, true, false);
        assert_eq!(
            (cursor.label, cursor.ground),
            (theme::TEXT_STRONG, Some(fill))
        );
        let delete = MenuItem::new("Delete thread").destructive();
        assert_eq!(row_inks(&delete, false, false).label, theme::TEXT);
        assert_eq!(row_inks(&delete, true, false).label, theme::TEXT_STRONG);
        let armed = row_inks(&delete, false, true);
        assert_eq!((armed.label, armed.ground), (theme::BLOCKED, Some(fill)));
        let dead = MenuItem::new("Reveal").disabled(true);
        let inks = row_inks(&dead, true, false);
        assert_eq!((inks.label, inks.ground), (theme::TEXT_MUTED, None));
        // TEXT_FAINT is never text, in any state.
        for inks in [row_inks(&rest, false, false), cursor, armed, inks] {
            assert!(![inks.label, inks.detail, inks.shortcut].contains(&theme::TEXT_FAINT));
        }
        assert_eq!(MenuItem::new("x").shortcut("").shortcut, None);
        assert!(match_highlights(std::slice::from_ref(&(0..2)), true).is_empty());
        assert_eq!(
            match_highlights(std::slice::from_ref(&(0..2)), false)[0]
                .1
                .font_weight,
            None
        );
    }

    #[test]
    fn a_menu_row_is_one_row_high_and_only_live_rows_point() {
        let mut row = menu_row("r", &MenuItem::new("Rename"), false, false);
        assert_eq!(row.style().size.height, Some(px(theme::MENU_ROW_H).into()));
        assert_eq!(
            row.style().mouse_cursor,
            Some(gpui::CursorStyle::PointingHand)
        );
        let mut dead = menu_row("d", &MenuItem::new("Reveal").disabled(true), false, false);
        assert_eq!(dead.style().mouse_cursor, None);
        assert_eq!(code_column_w(1), theme::MENU_NAME_MIN_W);
        assert_eq!(code_column_w(400), theme::MENU_NAME_MAX_W);
    }

    #[test]
    fn a_choice_draws_in_the_menu_grammar() {
        let choice = Choice {
            label: "Opus 5.5".into(),
            icon: Some((icons::CLAUDE, theme::PROVIDER_CLAUDE)),
            checked: true,
            detail: Some("1M".into()),
            ..Default::default()
        };
        let item = choice.item(false);
        assert!(item.checked && !item.disabled);
        assert_eq!(item.leading, None, "the mark rides the section title");
        assert_eq!(item.detail, Some("1M".into()));
        assert_eq!(
            choice.item(true).leading,
            Some((icons::CLAUDE, theme::PROVIDER_CLAUDE))
        );
        let dead = Choice {
            label: "Sonnet 5".into(),
            disabled: true,
            ..Default::default()
        };
        assert_eq!(
            row_inks(&dead.item(true), false, false).label,
            theme::TEXT_MUTED
        );
    }

    #[test]
    fn the_cursor_opens_on_the_standing_choice_past_sections() {
        let row = |label: &str, checked| Choice {
            label: label.to_string().into(),
            checked,
            ..Default::default()
        };
        let section = |label: &str| Choice {
            label: label.to_string().into(),
            section: true,
            ..Default::default()
        };
        // Items: Claude, Sonnet, Opus, Codex, GPT. First press → item 0.
        let menu = [
            section("Claude"),
            row("Sonnet", false),
            row("Opus", false),
            section("Codex"),
            row("GPT", false),
        ];
        assert_eq!(cursor_steps(&menu), 2, "0 → Sonnet");
        let mut picked = menu.clone();
        picked[4].checked = true;
        assert_eq!(cursor_steps(&picked), 4, "0 → Sonnet → Opus → GPT");
        assert_eq!(cursor_steps(&[row("a", false), row("b", true)]), 2);
        assert_eq!(cursor_steps(&[row("a", false)]), 1);
    }

    /// The loops' frame arithmetic: ten frames a turn, wrapping, never out
    /// of range.
    #[test]
    fn a_loop_shows_one_frame_per_tick_of_its_turn() {
        assert_eq!(frame_at(0.0, 10), 0);
        assert_eq!(frame_at(0.099, 10), 0);
        assert_eq!(frame_at(0.1, 10), 1);
        assert_eq!(frame_at(0.95, 10), 9);
        assert_eq!(frame_at(1.0, 10), 0, "a turn wraps");
        assert_eq!(frame_at(0.9999999, 10), 9);
        assert_eq!(icons::BRAILLE_FRAMES.len(), BRAILLE_FRAMES.len());
        assert_eq!(icons::WORKING_FRAMES.len(), WORKING_FRAMES.len());
        // The working cycle goes out and back, and holds ✻ when still.
        assert_eq!(WORKING_FRAMES[4], '\u{273b}');
        assert_eq!(icons::WORKING_FRAMES[4], icons::WORKED);
        for (out, back) in [(1, 9), (2, 8), (3, 7), (4, 6)] {
            assert_eq!(WORKING_FRAMES[out], WORKING_FRAMES[back]);
            assert_eq!(icons::WORKING_FRAMES[out], icons::WORKING_FRAMES[back]);
        }
        // 80ms and 120ms frames, as the prototype ticks them.
        assert_eq!(theme::MOTION_BRAILLE_FRAME_MS * 10, 800);
        assert_eq!(theme::MOTION_WORKING_FRAME_MS * 10, 1_200);
    }

    /// The shimmer's crest travels left to right across the run, peaks at
    /// the crest colour, and leaves the rest of the run in its base colour.
    #[test]
    fn the_shimmer_crest_sweeps_left_to_right() {
        let base: Hsla = rgb(theme::PROVIDER_CLAUDE).into();
        let crest = shimmer_crest(theme::PROVIDER_CLAUDE);
        let text = "Reticulating\u{2026}";
        // Before and after the sweep: nothing is lit.
        assert!(shimmer_highlights(text, base, crest, 0.0).is_empty());
        assert!(shimmer_highlights(text, base, crest, 0.999).len() <= 1);
        // Mid-sweep: the crest sits at the run's middle, symmetric.
        let mid = shimmer_highlights(text, base, crest, 0.5);
        assert!(!mid.is_empty());
        let lit = |phase: f32| -> Vec<usize> {
            shimmer_highlights(text, base, crest, phase)
                .iter()
                .map(|(range, _)| range.start)
                .collect()
        };
        let early = lit(0.35);
        let late = lit(0.65);
        assert!(early.first() < late.first(), "{early:?} then {late:?}");
        // Ranges are byte ranges over whole characters, in order.
        for pair in mid.windows(2) {
            assert!(pair[0].0.end <= pair[1].0.start);
        }
        let last = mid.last().unwrap().0.clone();
        assert!(text.is_char_boundary(last.start) && text.is_char_boundary(last.end));
        // The crest is lighter than the base, and no highlight is brighter.
        assert!(crest.l > base.l);
        for (_, style) in &mid {
            assert!(style.color.unwrap().l <= crest.l + 1e-4);
        }
        assert!(shimmer_highlights("", base, crest, 0.5).is_empty());
    }

    /// The caret's soft blink: full, eased down, held low, eased back.
    #[test]
    fn the_caret_blinks_softly() {
        assert_eq!(caret_blink(0.0), 1.0);
        assert_eq!(caret_blink(0.44), 1.0);
        assert_eq!(caret_blink(0.6), theme::CARET_BLINK_MIN);
        assert_eq!(caret_blink(0.9), theme::CARET_BLINK_MIN);
        let falling = caret_blink(0.5);
        assert!(theme::CARET_BLINK_MIN < falling && falling < 1.0);
        assert!((caret_blink(0.9999) - 1.0).abs() < 0.01);
        assert_eq!(caret_blink(1.0), 1.0, "a period wraps");
    }

    /// In tests the kit rests (reduced motion): each loop draws its still
    /// state and leases nothing, so a board of spinners schedules no frame.
    #[gpui::test]
    fn spinners_and_the_shimmer_hold_still_at_rest(cx: &mut gpui::TestAppContext) {
        struct Board;
        impl gpui::Render for Board {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                div()
                    .child(braille_spinner(theme::RUNNING, theme::GLYPH_BOX))
                    .child(working_spinner(theme::PROVIDER_CLAUDE, theme::GLYPH_BOX))
                    .child(shimmer("Working", theme::PROVIDER_CLAUDE))
            }
        }
        let window = cx.add_window(|_, _| Board);
        window
            .update(cx, |_, window, cx| {
                window.refresh();
                assert_eq!(caret_opacity(window, cx), 1.0);
            })
            .unwrap();
        cx.run_until_parked();
        cx.update(|cx| assert!(motion::pulse_parked(cx), "no loop leased the clock"));
    }
}
