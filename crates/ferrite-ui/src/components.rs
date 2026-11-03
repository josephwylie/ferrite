//! Ferrite's shared primitives: the one way render code opens a text run,
//! lays a plane or a floating surface, draws a status mark or a shortcut,
//! builds a control or a menu row, and runs the few loops the terminal
//! grammar allows (the working mark, the shimmer, the caret
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
    HighlightStyle, Hsla, SharedString, StyleRefinement, Window,
};

use crate::icons;
use crate::motion;
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
    focus_outline(style, rgba(theme::FOCUS_RING).into())
}

fn focus_outline(style: StyleRefinement, ink: Hsla) -> StyleRefinement {
    style.shadow(vec![BoxShadow {
        inset: true,
        color: ink,
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
/// `rest → hover` over `motion::HOVER_FADE` (Claude's 60ms) under a key
/// derived from its id, the kit's own hover is held at that ground
/// (`pointer::button_variant`) so it never snaps over the blend, and the
/// press lands at once on `press` while the button squishes and springs
/// back (`motion::pressable`). The caller sets no ground of its own after
/// it.
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
    let button = button(id)
        .custom(crate::pointer::button_variant(ground, ink, press, cx))
        .when(ground.a > 0.0, |button| button.bg(ground))
        .on_hover(motion::hover_listener(key.clone()));
    motion::pressable(button, key)
}

/// A text tooltip on a faded kit `Button`, in the floating vocabulary
/// (`menu::tooltip`). The kit's own `tooltip` rides an `on_hover` listener,
/// and an element carries one: the blend's. This one is gpui's own.
pub trait Tip {
    fn tip(self, text: impl Into<SharedString>) -> Self;
    /// The verb and its key (`menu::tooltip_with_key`): `Open agent ⏎`.
    fn tip_with_key(self, label: impl Into<SharedString>, key: Option<String>) -> Self;
}

impl Tip for Button {
    fn tip(mut self, text: impl Into<SharedString>) -> Self {
        self.interactivity().tooltip(crate::menu::tooltip(text));
        self
    }

    fn tip_with_key(mut self, label: impl Into<SharedString>, key: Option<String>) -> Self {
        self.interactivity()
            .tooltip(crate::menu::tooltip_with_key(label, key));
        self
    }
}

/// A form action: nothing at rest, `paint::HOVER` under the pointer
/// (blended), `paint::PRESS` pressed.
pub fn form_button(id: impl Into<ElementId>, cx: &App) -> Button {
    form_button_on(id, theme::Paint(theme::TRANSPARENT), cx)
}

/// `form_button` resting on its own `rest` ground (a filled field-like
/// control): the same blend and press. A filled rest steps to `RAISED`;
/// the hover overlay would replace, not lie over, its own ground.
pub fn form_button_on(id: impl Into<ElementId>, rest: theme::Paint, cx: &App) -> Button {
    let hover = if rest.is_opaque() {
        theme::paint::RAISED
    } else {
        theme::paint::HOVER
    };
    faded_button(
        id,
        rest.into(),
        hover.into(),
        theme::paint::PRESS.into(),
        rgb(theme::INK_STRONG).into(),
        cx,
    )
    .tab_stop(true)
}

/// The completing action: the strongest ink as a fill (`PRIMARY`) with
/// dark `ON_PRIMARY` ink, stepping down to `PRIMARY_HOVER` under the pointer and
/// pressed; disabled is `SELECTION` with `INK_SUBTLE` ink. On the filled face
/// the focus outline is `INK_STRONG`.
pub fn primary_button(id: impl Into<ElementId>, disabled: bool, cx: &App) -> Button {
    use gpui::component::Disableable;
    let face: Hsla = rgb(primary_face(disabled)).into();
    let (hover, press) = if disabled {
        (face, face)
    } else {
        (
            rgb(theme::PRIMARY_HOVER).into(),
            rgb(theme::PRIMARY_HOVER).into(),
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
    .focus_visible(primary_focus)
    .disabled(disabled)
    .when(disabled, |button| button.cursor_default())
}

/// A primary button's keyboard focus: the outline in `INK_STRONG`, which
/// reads on its steel face where `FOCUS_RING` would not.
fn primary_focus(style: StyleRefinement) -> StyleRefinement {
    focus_outline(style, rgb(theme::INK_STRONG).into())
}

fn primary_face(disabled: bool) -> u32 {
    if disabled {
        theme::SELECTED
    } else {
        theme::PRIMARY
    }
}

fn primary_ink(disabled: bool) -> u32 {
    if disabled {
        theme::INK_SUBTLE
    } else {
        theme::ON_PRIMARY
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
/// `INK_STRONG`.
pub fn text_ui() -> Div {
    div()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(theme::INK_STRONG))
}

/// Metadata: the same grid line in `INK_SUBTLE` (one size per surface:
/// metadata is ink, not a smaller size).
pub fn text_meta() -> Div {
    div()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(theme::INK_SUBTLE))
}

/// A group's title inside a surface: the grid's type, `W_LABEL` `INK_SUBTLE`,
/// written as-is (terminal case, no rule), 8px above what it heads.
pub fn section_label(text: impl Into<SharedString>) -> Div {
    text_meta()
        .font_weight(theme::W_LABEL)
        .pb(px(theme::SPACE_2))
        .child(text.into())
}

/// Mono words in exactly their run's width (`run_width`: `chars × CH` at
/// the UI size, to the layout unit), as the browser lays a span of them.
pub fn cells(text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    div()
        .flex_shrink_0()
        .w(px(run_width(&text)))
        .whitespace_nowrap()
        .child(text)
}

/// `text`'s width on the UI grid: one `CH` a character.
pub fn cells_width(text: &str) -> f32 {
    text.chars().count() as f32 * theme::CH
}

/// `text`'s width as one run, as the browser holds it: its cells, rounded
/// up to the layout unit (1/64px) a measured run takes.
pub fn run_width(text: &str) -> f32 {
    (cells_width(text) * 64.0).ceil() / 64.0
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
    #[cfg_attr(not(test), allow(dead_code))]
    Pane,
    /// The focused Pane of a board: flat (its border says focus).
    #[cfg_attr(not(test), allow(dead_code))]
    Lifted,
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

/// A line's right-end fade, `LINE_FADE` wide, from clear to `fill` — the
/// ground under the line — so text cut at a box's right edge fades out
/// rather than ending in "…" (Claude's sidebar mask). The caller lays it
/// in a `relative` box over the clipped line; it answers no pointer.
pub fn line_fade(fill: Hsla) -> Div {
    div()
        .absolute()
        .top_0()
        .right_0()
        .h_full()
        .w(px(theme::LINE_FADE))
        .bg(gpui::linear_gradient(
            90.,
            gpui::linear_color_stop(Hsla { a: 0., ..fill }, 0.),
            gpui::linear_color_stop(fill, 1.),
        ))
}

/// A rung's shadow: nothing, except a floating surface's one small shadow
/// (`float_shadow`).
pub fn elevation(rung: Elevation) -> Vec<BoxShadow> {
    match rung {
        Elevation::Pane | Elevation::Lifted => Vec::new(),
        Elevation::Float | Elevation::Sheet => float_shadow(),
    }
}

/// An edge fade, `EDGE_FADE` tall, in `fill` — the surface it lies on:
/// solid over its first half (a line's cap height, so a row cut at the edge
/// is hidden, not dimmed), then easing out: held near-solid
/// (`EDGE_FADE_KNEE`) over the next quarter, where the rest of a cut line
/// falls, and gone by the far edge. gpui's gradient has two stops, so the
/// ease is two of them stacked. `rising` runs it up from the foot. It
/// answers no pointer; the caller lays it absolute over the scroller's
/// edge.
pub fn edge_fade(fill: Hsla, rising: bool) -> Div {
    let at = |a: f32| Hsla { a, ..fill };
    let angle = if rising { 0. } else { 180. };
    let quarter = (theme::EDGE_FADE / 4.).round();
    let hold = div().w_full().h(px(2. * quarter)).bg(fill);
    let knee = div().w_full().h(px(quarter)).bg(gpui::linear_gradient(
        angle,
        gpui::linear_color_stop(at(1.), 0.),
        gpui::linear_color_stop(at(theme::EDGE_FADE_KNEE), 1.),
    ));
    let ease = div().w_full().flex_1().bg(gpui::linear_gradient(
        angle,
        gpui::linear_color_stop(at(theme::EDGE_FADE_KNEE), 0.),
        gpui::linear_color_stop(at(0.), 1.),
    ));
    let fade = div().flex().flex_col().w_full().h(px(theme::EDGE_FADE));
    if rising {
        fade.child(ease).child(knee).child(hold)
    } else {
        fade.child(hold).child(knee).child(ease)
    }
}

/// `shadow` with its ink scaled by `k` (0..1): a floating surface's shadow
/// while it fades in or out (`motion::menu_in`).
pub fn shadow_faded(shadow: Vec<BoxShadow>, k: f32) -> Vec<BoxShadow> {
    shadow
        .into_iter()
        .map(|layer| BoxShadow {
            color: layer.color.opacity(k.clamp(0.0, 1.0)),
            ..layer
        })
        .collect()
}

/// The one cast shadow (theme rule 2): CSS `0 6px 14px` at 50%, under
/// every floating surface but the palette's dialog (which deepens it,
/// `palette::dialog`) and nothing else.
pub fn float_shadow() -> Vec<BoxShadow> {
    vec![layer(
        theme::SHADOW_FLOAT,
        theme::SHADOW_FLOAT_Y,
        theme::SHADOW_FLOAT_BLUR,
        0.,
    )]
}

/// The palette's dialog's shadows (SPEC §5.8): its 1px `FLOAT_EDGE`
/// ring as a spread, then the float's shadow deepened (`SHADOW_PALETTE_*`).
/// The dialog clips its rows to its radius, which would clip an
/// `outer_ring` overlay, so its ring is a shadow: the same 1px outside.
pub fn dialog_shadow() -> Vec<BoxShadow> {
    vec![
        layer(theme::paint::FLOAT_EDGE.rgba(), 0., 0., theme::FOCUS_RING_W),
        layer(
            theme::SHADOW_PALETTE,
            theme::SHADOW_PALETTE_Y,
            theme::SHADOW_PALETTE_BLUR,
            0.,
        ),
    ]
}

/// **The outer ring**: a 1px ring in `ink` just outside its parent's box of
/// `radius` (the mockups' `box-shadow: 0 0 0 1px`), an overlay
/// `FOCUS_RING_W` past every edge, rounded to match. It takes no layout and
/// no pointer, so the box keeps its size and a state change moves nothing.
/// The parent is `relative` and does not clip. A Pane's card
/// (`pane::card_ring`), the Solo and the compact Composer wear it.
pub fn outer_ring(radius: f32, ink: impl Into<Hsla>) -> Div {
    let out = px(-theme::FOCUS_RING_W);
    div()
        .absolute()
        .top(out)
        .left(out)
        .right(out)
        .bottom(out)
        .rounded(px(radius + theme::FOCUS_RING_W))
        .border_1()
        .border_color(ink.into())
}

/// A floating surface (menu, picker, popover, card, toast, sheet; SPEC
/// §5.8's shell at the float's scale): the float's opaque ground
/// (`paint::FLOAT`; floats cannot frost, theme WP-E FL-15), radius
/// `R_FLOAT`, a 1px `FLOAT_EDGE` ring, the float shadow, `FLOAT_PAD`
/// inside, the UI type. It occludes what it covers and owns its cursor.
/// The caller states its width and position. (The palette's dialog is the
/// same shell at its own scale, `palette::dialog`.)
pub fn floating_surface() -> Div {
    float_ground().cursor_default().occlude()
}

/// The floating surface's look alone, answering no pointer: a float on its
/// way out (a toast fading away) must not hold a hit area it no longer
/// stands behind.
pub fn float_ground() -> Div {
    text_ui()
        .flex()
        .flex_col()
        .p(px(theme::FLOAT_PAD))
        .rounded(px(theme::R_FLOAT))
        .bg(theme::paint::FLOAT)
        .border_1()
        .border_color(theme::paint::FLOAT_EDGE)
        .shadow(float_shadow())
}

/// The modal veil: covers its parent, takes every press, centres its child.
pub fn veil() -> Div {
    veil_frame().occlude().bg(rgba(theme::VEIL))
}

/// The veil's box alone — covering its parent, centring its child — with no
/// ground and no hit area: where a sheet that went fades out
/// (`motion::veil_out`), letting presses through.
pub fn veil_frame() -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
}

// ------------------------------------------------------------- the line

/// Where the browser paints a box edge at `x` (a layout position in
/// logical pixels): on the whole pixel, half up — what Ferrite's gpui does
/// with every laid-out box (vendor/gpui-pre, "Pixel snapping"), for edges a
/// custom element paints itself.
pub fn css_px(x: f32) -> f32 {
    (x + 0.5).floor()
}

// ------------------------------------------------------------------ marks

/// A glyph set as the prototype's browser sets it: text, in the line's
/// face, size and weight, on the line's baseline, the platform's font
/// fallback supplying what Geist Mono lacks — on macOS Menlo for
/// `❯ ◆ ✓ ✗ ▾ ▸ ⌘ ⌥ ⌃ ✢ ✳ ✶ ✻ ✽`, STIX Two Math for `⏵ ⏸`
/// (theme rule 10). It starts where its cell does;
/// its advance is its face's.
pub fn glyph(text: impl Into<SharedString>, ink: u32) -> Div {
    div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .text_color(rgb(ink))
        .child(text.into())
}

/// A status dot: Geist Mono's `●` (the prototype's `.dot`), set as text.
pub fn status_dot(ink: u32) -> Div {
    glyph("\u{25cf}", ink)
}

/// A hollow status dot (parked, the prototype's `○`), set as text.
pub fn status_ring(ink: u32) -> Div {
    glyph("\u{25cb}", ink)
}

// ------------------------------------------------------------------ loops
//
// The few loops the terminal grammar allows (theme rule 8). Each rides the
// shared pulse clock (`motion::loop_phase`: one timer on a ~30fps grid for
// the whole window, parked when nothing loops), declaring how its picture
// moves so the view that paints it is drawn only when it changes, and holds
// a static end state under reduced motion that says the same thing.

pub use loops::*;

/// The loops (see the section note above): re-exported, so callers name
/// `components::working_mark` and friends.
mod loops {
    use super::*;

    /// Which of `frames` a loop shows at `phase` [0, 1) of its turn: the
    /// cadence's own count, so the frame drawn and the step the pulse clock
    /// wakes for never disagree.
    pub fn frame_at(phase: f32, frames: usize) -> usize {
        ferrite_core::cadence::frame_at(phase, frames)
    }

    /// The app's one running glyph (theme rule 8): four white orbs
    /// swapping corners in pairs (`working_mark`). It marks
    /// a working Thread (the nav, Pane heads, subagent tabs), the working
    /// line, a plan's current step and a running check, so a board of them
    /// shares one clock.
    pub use crate::working_mark::working_mark;

    /// The colour the shimmer crests at over `base`: Claude's clay crests at
    /// the prototype's `#ffe1d3`; another ink lifted toward white by
    /// `SHIMMER_LIFT`.
    pub fn shimmer_crest(base: u32) -> Hsla {
        if base == theme::PROVIDER_CLAUDE {
            rgb(theme::SHIMMER_CREST_CLAUDE).into()
        } else {
            motion::mix(rgb(base).into(), rgb(0xffffff).into(), theme::SHIMMER_LIFT)
        }
    }

    /// How far toward its crest the shimmer lifts the ink at `x` (a
    /// fraction of the run's width) at `phase` [0, 1): the prototype's 300%
    /// linear gradient — a crest `2 × SHIMMER_HALF_WIDTH` of the run wide,
    /// linear on each side, travelling left to right from half a run
    /// before the text to half a run past it. 0 off the crest.
    pub fn shimmer_lift(x: f32, phase: f32) -> f32 {
        let centre = -0.5 + 2.0 * phase.rem_euclid(1.0);
        (1.0 - (x - centre).abs() / theme::SHIMMER_HALF_WIDTH).max(0.0)
    }

    /// The working caption's shimmer (theme rule 8): `text` in `base` (the
    /// provider's colour) under a crest of `shimmer_crest(base)` sweeping left
    /// to right every `MOTION_SHIMMER_MS`, its ink graded across the run as
    /// the prototype's text-clipped gradient grades it: per device column,
    /// not per character (`ShimmerText`). One run, so nothing reflows; one
    /// line that truncates at its end. Only the focused Pane's working line
    /// wears it; under reduced motion it is plain `base`. The caller sets
    /// the face, size and line height.
    pub fn shimmer(text: impl Into<SharedString>, base: u32) -> AnyElement {
        let shimmer = Shimmer {
            text: text.into(),
            base,
        };
        crate::loops_overlay::hosted(move || shimmer.clone().into_any_element()).into_any_element()
    }

    #[derive(IntoElement, Clone)]
    struct Shimmer {
        text: SharedString,
        base: u32,
    }

    impl RenderOnce for Shimmer {
        fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
            let base: Hsla = rgb(self.base).into();
            // The crest moves at every instant: a frame loop at the
            // display's rate, as the orbs beside it are.
            let turn = Duration::from_millis(theme::MOTION_SHIMMER_MS);
            let frame = motion::frame_loop(turn, motion::LoopClock::Css, cx);
            let lit = frame.map(|frame| (frame.phase, shimmer_crest(self.base)));
            let leases = frame.is_some_and(|frame| frame.leases);
            div()
                .min_w_0()
                .truncate()
                .text_color(base)
                .debug_selector(|| "shimmer".into())
                .child(ShimmerText::new(self.text, base, lit, leases))
        }
    }

    /// A run in `base`, or, while it shimmers, its glyphs painted a device
    /// column at a time in the gradient's ink there (a glyph's columns
    /// each take their own ink, as the browser's clipped gradient does).
    /// Either way the run is laid out by its `StyledText`, which truncates
    /// it with `…` to the line's width; the shimmer paints that same
    /// shaped line, so a narrow Pane cuts it where the still run is cut.
    struct ShimmerText {
        base: Hsla,
        /// The crest's phase and colour; `None` holds the run still.
        lit: Option<(f32, Hsla)>,
        /// Whether this draw asks for the next frame (`motion::lease_frame`).
        leases: bool,
        child: AnyElement,
        layout: gpui::TextLayout,
    }

    #[cfg(test)]
    thread_local! {
        /// The text the shimmer last painted lit.
        pub(super) static SHIMMER_PAINTED: std::cell::RefCell<Option<String>> =
            const { std::cell::RefCell::new(None) };
    }

    impl ShimmerText {
        fn new(text: SharedString, base: Hsla, lit: Option<(f32, Hsla)>, leases: bool) -> Self {
            let text = gpui::StyledText::new(text);
            let layout = text.layout().clone();
            Self {
                base,
                lit,
                leases,
                child: text.into_any_element(),
                layout,
            }
        }
    }

    impl IntoElement for ShimmerText {
        type Element = Self;

        fn into_element(self) -> Self::Element {
            self
        }
    }

    impl gpui::Element for ShimmerText {
        type RequestLayoutState = ();
        type PrepaintState = ();

        fn id(&self) -> Option<ElementId> {
            None
        }

        fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
            None
        }

        fn request_layout(
            &mut self,
            _: Option<&gpui::GlobalElementId>,
            _: Option<&gpui::InspectorElementId>,
            window: &mut Window,
            cx: &mut App,
        ) -> (gpui::LayoutId, ()) {
            (self.child.request_layout(window, cx), ())
        }

        fn prepaint(
            &mut self,
            _: Option<&gpui::GlobalElementId>,
            _: Option<&gpui::InspectorElementId>,
            bounds: gpui::Bounds<gpui::Pixels>,
            _: &mut (),
            window: &mut Window,
            cx: &mut App,
        ) {
            // The next frame, only while the crest can be seen.
            if self.leases && window.content_mask().bounds.intersects(&bounds) {
                motion::lease_frame(window, cx);
            }
            self.child.prepaint(window, cx);
        }

        fn paint(
            &mut self,
            _: Option<&gpui::GlobalElementId>,
            _: Option<&gpui::InspectorElementId>,
            bounds: gpui::Bounds<gpui::Pixels>,
            _: &mut (),
            _: &mut (),
            window: &mut Window,
            cx: &mut App,
        ) {
            let Some((phase, crest)) = self.lit else {
                self.child.paint(window, cx);
                return;
            };
            // The line the run laid out as: truncated, `…` and all.
            let Some(line) = self
                .layout
                .line_layouts()
                .first()
                .map(|line| line.unwrapped_layout.clone())
            else {
                return;
            };
            #[cfg(test)]
            SHIMMER_PAINTED.with(|painted| painted.replace(Some(self.layout.text())));
            let size = line.font_size;
            let line_height = self.layout.line_height();
            let width = f32::from(line.width);
            if width <= 0.0 {
                return;
            }
            let scale = window.scale_factor();
            let left = f32::from(bounds.left());
            let right = f32::from(bounds.right()).min(left + width);
            let baseline =
                bounds.top() + (line_height - line.ascent - line.descent) / 2. + line.ascent;

            let glyphs: Vec<_> = line
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.font_id, glyph)))
                .collect();
            let advance = f32::from(size);
            let first = (left * scale).floor() as i32;
            let last = (right * scale).ceil() as i32;
            for column in first..last {
                let x0 = column as f32 / scale;
                let x1 = (column + 1) as f32 / scale;
                let lift = shimmer_lift(((x0 + x1) / 2.0 - left) / width, phase);
                let ink = motion::mix(self.base, crest, lift);
                let mask = gpui::Bounds::from_corners(
                    gpui::point(px(x0), bounds.top()),
                    gpui::point(px(x1), bounds.bottom()),
                );
                window.with_content_mask(Some(gpui::ContentMask { bounds: mask }), |window| {
                    for (font_id, glyph) in &glyphs {
                        let at = left + f32::from(glyph.position.x);
                        if at > x1 || at + advance < x0 {
                            continue;
                        }
                        let _ = window.paint_glyph(
                            gpui::point(px(at), baseline),
                            *font_id,
                            glyph.id,
                            size,
                            ink,
                        );
                    }
                });
            }
        }
    }

    /// How `caret_blink` moves: two equal steps a turn, shown then hidden,
    /// so the pulse clock draws it twice a turn and nothing between.
    pub const CARET_STEPS: motion::Motion<'static> = motion::Motion::Frames(2);

    /// The caret's opacity at `phase` [0, 1) of `MOTION_CARET_BLINK_MS`: a
    /// hard blink, shown for the first half of the turn and hidden for the
    /// second, as both reference apps blink theirs.
    pub fn caret_blink(phase: f32) -> f32 {
        if frame_at(phase, 2) == 0 {
            1.0
        } else {
            0.0
        }
    }

    /// The focused Composer's caret opacity now, declaring the painting view
    /// on the pulse clock through the blink's fades; full and still under
    /// reduced motion. An unfocused Composer draws no blinking caret (its
    /// caret is a faint outline, still).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn caret_opacity(window: &mut Window, cx: &mut App) -> f32 {
        if motion::reduced_motion(cx) {
            return 1.0;
        }
        let turn = Duration::from_millis(theme::MOTION_CARET_BLINK_MS);
        caret_blink(motion::css_phase(
            turn,
            CARET_STEPS,
            window.current_view(),
            cx,
        ))
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

/// A modifier's glyph as a key combination spells it: `cmd` ⌘, `shift` ⇧,
/// `alt` ⌥, `ctrl` ⌃. `None` for a key that is its own word.
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
#[cfg(any(test, feature = "test-support"))]
pub fn key_glyphs(keys: &str) -> String {
    let glyph = |part: &str| key_glyph(part).map_or_else(|| part.to_string(), String::from);
    keys.split(' ')
        .map(|word| word.split('-').map(glyph).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One stroke of a key table's spelling, split into its parts: the
/// modifiers, then the key — `cmd-shift-P` → `cmd`, `shift`, `P`; `cmd--`
/// → `cmd`, `-` (the minus key is itself a dash).
pub fn chord_parts(stroke: &str) -> Vec<&str> {
    let (modifiers, key) = match stroke.strip_suffix("--") {
        Some(head) => (Some(head), "-"),
        None if stroke == "-" => (None, "-"),
        None => match stroke.rsplit_once('-') {
            Some((head, key)) => (Some(head), key),
            None => (None, stroke),
        },
    };
    let mut parts: Vec<&str> = modifiers
        .map(|head| head.split('-').filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    parts.push(key);
    parts
}

/// A key combination as it is set, from a key table's spelling, in the
/// code face (keys are machine text, rule 6): one run of text, as the
/// prototype types it (`⌘⇧N`, `⌘D`) — `⌘` `⌥` `⌃` in the face the platform
/// falls back to (`glyph`), `⇧` and every key word (`⌫` `⏎` `⇥`, a letter)
/// Geist Mono's own. Parts joined by `-` sit tight, as a menu shortcut or a
/// tooltip reads (`cmd-F` → `⌘F`); strokes joined by spaces keep one code
/// space apart. The run names its modifier for the tests
/// (`command-key`, …).
pub fn key_combo(keys: &str, ink: u32) -> Div {
    let text = chord_text(keys);
    let selector = if text.contains('\u{2318}') {
        "command-key"
    } else if text.contains('\u{2325}') {
        "option-key"
    } else if text.contains('\u{2303}') {
        "control-key"
    } else if text.contains('\u{21e7}') {
        "shift-key"
    } else {
        "key-combo"
    };
    glyph(SharedString::from(text), ink)
        .font_family(theme::FONT_CODE)
        .debug_selector(move || selector.into())
}

/// One key on its own cap (Claude's `Esc` in its search's footer, its
/// `⌘` `↩` after `Allow once`): the glyph in `FS_META` on a `KEY_CAP`
/// square, at least, at `KEY_CAP_R`, edged 1px in `edge`, no fill.
pub fn key_cap(key: impl Into<SharedString>, ink: impl Into<Hsla>, edge: impl Into<Hsla>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .min_w(px(theme::KEY_CAP))
        .h(px(theme::KEY_CAP))
        .px(px(theme::SPACE_0_5))
        .border_1()
        .border_color(edge.into())
        .rounded(px(theme::KEY_CAP_R))
        .text_size(px(theme::FS_META))
        .line_height(px(theme::KEY_CAP))
        .text_color(ink.into())
        .child(key.into())
}

/// A chord as its glyphs, one stroke after another: `cmd-k` → `⌘K`.
pub fn chord_text(keys: &str) -> String {
    keys.split(' ')
        .map(|stroke| {
            chord_parts(stroke)
                .into_iter()
                .map(|part| key_glyph(part).map_or_else(|| part.to_string(), String::from))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The prompt mark `❯` (Geist Mono lacks U+276F), set as text at the
/// semibold weight in one character cell, as the prototype's `.g` sets it:
/// the face the platform falls back to draws it (`glyph`). The transcript
/// prompt, the Composer, the nav cursor and every selection bar share it;
/// `ink` is `INK_STRONG` where it marks the live input or the selected row,
/// `INK_SUBTLE` where it does not.
pub fn prompt_mark(ink: u32) -> AnyElement {
    div()
        .flex_shrink_0()
        .w(px(theme::CH))
        .font_weight(theme::W_LABEL)
        .child(glyph("\u{276f}", ink))
        .into_any_element()
}

/// An on/off switch's face (theme `SWITCH_*`): a pill track, `PRIMARY` on
/// and `SWITCH_OFF` off, its round knob at the end that says which —
/// `ON_PRIMARY` on, `INK_SUBTLE` off. A flip slides the knob over
/// `motion::SWITCH` (a slight overshoot, `motion::settled`) while the track
/// and the knob's ink take the new state at once, as Claude's do; under
/// reduced motion the knob lands at once too. The caller makes it a control
/// (a tab stop that flips on a press, Enter or Space).
pub fn switch(id: impl Into<ElementId>, on: bool) -> impl IntoElement {
    motion::settled(id, on, motion::SWITCH, move |t| {
        let knob = theme::SWITCH_H - 2.0 * theme::SWITCH_INSET;
        let (track, ink): (Hsla, u32) = if on {
            (rgb(theme::PRIMARY).into(), theme::ON_PRIMARY)
        } else {
            (theme::SWITCH_OFF.into(), theme::INK_SUBTLE)
        };
        div()
            .relative()
            .flex_shrink_0()
            .w(px(theme::SWITCH_W))
            .h(px(theme::SWITCH_H))
            .rounded_full()
            .bg(track)
            .child(
                div()
                    .absolute()
                    .top(px(theme::SWITCH_INSET))
                    .left(px(
                        theme::SWITCH_INSET + t * (theme::SWITCH_W - theme::SWITCH_H)
                    ))
                    .size(px(knob))
                    .rounded_full()
                    .bg(rgb(ink)),
            )
    })
}

/// The words the home greets with: the empty board's heading.
pub const GREETING: &str = "What should we work on?";

/// The home's heading (theme `HOME_*`): the steel mark over the greeting,
/// centred, on the empty board.
pub fn greeting() -> Div {
    div()
        .debug_selector(|| "greeting".into())
        .flex()
        .flex_col()
        .items_center()
        .flex_shrink_0()
        .gap(px(theme::HOME_MARK_GAP))
        .child(steel_mark(theme::HOME_MARK_H))
        .child(
            div()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_HOME_TITLE))
                .line_height(px(theme::LH_HOME_TITLE))
                .font_weight(theme::W_LABEL)
                .text_color(rgb(theme::INK_STRONG))
                .whitespace_nowrap()
                .child(GREETING),
        )
}

/// Ferrite's steel mark, `height` tall, cropped to the shards themselves —
/// the prototype's `viewBox="280 30 700 1130"` (the two paths of the
/// 1254-unit mark, the `#C7D5E8 → #9FB3D0 → #748EBA` gradient, a 26-unit
/// stroke with round joins) — so what follows it starts the gap after the
/// mark, not after its box. The empty board's banner draws it `HOME_MARK_H`
/// tall.
///
/// The browser draws an inline `<svg>` at its own size from its box's
/// rounded corner; an image at the mark's own 700 × 1130 would be shrunk by
/// the GPU without filtering and lose its antialiasing. So the mark is
/// rasterized at its device size, once per size (`SteelMark`).
pub fn steel_mark(height: f32) -> Div {
    // The shards span x 280..980 and y 30..1160 of the mark's 1254 box; the
    // image is cropped to them and paints their steel gradient.
    let width = height * 700.0 / 1130.0;
    div()
        .flex_shrink_0()
        .w(px(width))
        .h(px(height))
        .child(SteelMark {
            width,
            height,
            child: None,
        })
}

/// The steel mark at `width` × `height` from its box's rounded corner.
struct SteelMark {
    width: f32,
    height: f32,
    child: Option<AnyElement>,
}

impl IntoElement for SteelMark {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for SteelMark {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, ()) {
        let mut style = gpui::Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = gpui::relative(1.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        // On a canvas of whole pixels (its box then rounds to itself), the
        // drawing at its own scale, the canvas's spare width blank.
        let scale = window.scale_factor();
        let canvas = gpui::size(px(self.width.ceil()), px(self.height.ceil()));
        let image = steel_image(
            (f32::from(canvas.width) * scale).round().max(1.) as u32,
            (f32::from(canvas.height) * scale).round().max(1.) as u32,
            f32::from(canvas.width) / self.width,
            f32::from(canvas.height) / self.height,
        );
        let mut child = gpui::img(gpui::ImageSource::Image(image))
            .w(canvas.width)
            .h(canvas.height)
            .into_any_element();
        child.layout_as_root(canvas.into(), window, cx);
        child.prepaint_at(bounds.origin, window, cx);
        self.child = Some(child);
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

/// The steel mark's drawing on a `width` × `height` device-pixel canvas
/// `spare` times the drawing's own width and height (the rest blank, right
/// and below); one image per size.
fn steel_image(width: u32, height: u32, spare_w: f32, spare_h: f32) -> std::sync::Arc<gpui::Image> {
    type Key = (u32, u32, u32, u32);
    thread_local! {
        static STEEL: std::cell::RefCell<std::collections::HashMap<Key, std::sync::Arc<gpui::Image>>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    const DRAWING: &str = include_str!("../assets/icons/ferrite-steel.svg");
    let key = (width, height, spare_w.to_bits(), spare_h.to_bits());
    STEEL.with(|cache| {
        cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let svg = DRAWING.replacen(
                    r#"viewBox="280 30 700 1130" width="700" height="1130""#,
                    &format!(
                        r#"viewBox="280 30 {} {}" width="{width}" height="{height}""#,
                        700.0 * spare_w,
                        1130.0 * spare_h
                    ),
                    1,
                );
                std::sync::Arc::new(gpui::Image::from_bytes(
                    gpui::ImageFormat::Svg,
                    svg.into_bytes(),
                ))
            })
            .clone()
    })
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
/// box (`first_line_h` high), so text starts at C1 on every row. (Its one
/// caller, the L2 tail, went with the L2 cell; the tests keep the recipe.)
#[cfg_attr(not(test), allow(dead_code))]
pub fn gutter(mark: impl IntoElement, first_line_h: f32) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(theme::GUTTER_W))
        .h(px(first_line_h))
        .child(glyph_box(mark))
}

// --------------------------------------------------------------- controls

/// The chord an action is bound to with no key context, or in the
/// Cockpit's own (`esc` to close), as a menu shortcut spells it (`cmd-D`,
/// the last key upper-cased; `esc`, `⏎`, `⌫`): `None` when nothing binds
/// it, so a tooltip never names a key that would not act.
pub fn bound_chord(action: &str) -> Option<String> {
    bound_chord_in(action, None).or_else(|| bound_chord_in(action, Some(crate::keymap::COCKPIT)))
}

/// The chord an action is bound to in exactly `context` (`None`: no
/// context), spelled as `bound_chord` spells it — for a key that only acts
/// somewhere (⌘⌫ parks from an empty line: `ComposerEmpty`).
pub fn bound_chord_in(action: &str, context: Option<&str>) -> Option<String> {
    let (keys, _, _) = crate::keymap::active()
        .into_iter()
        .find(|(_, bound, bound_context)| *bound == action && *bound_context == context)?;
    Some(spell_chord(&keys))
}

/// A key table's spelling as a combination draws it: the modifiers stay
/// words (`key_combo` draws them), the key becomes its keycap word
/// (`key_word`): `cmd-backspace` → `cmd-⌫`, `shift-tab` → `shift-⇥`.
pub fn spell_chord(keys: &str) -> String {
    keys.split(' ')
        .map(|stroke| {
            let mut parts: Vec<String> = chord_parts(stroke)
                .into_iter()
                .map(str::to_string)
                .collect();
            if let Some(key) = parts.last_mut() {
                *key = key_word(key);
            }
            parts.join("-")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A key table's key as a combination spells it: a letter upper-cased,
/// `escape` as `esc`, `enter` `⏎`, `backspace` `⌫`, `tab` `⇥`, `shift`
/// `⇧`, the minus key `−` and the arrows as arrows — the words a keycap
/// prints.
pub fn key_word(key: &str) -> String {
    match key {
        "escape" => "esc".into(),
        "enter" => "\u{23ce}".into(),
        "backspace" => "\u{232b}".into(),
        "tab" => "\u{21e5}".into(),
        "shift" => "\u{21e7}".into(),
        "-" => "\u{2212}".into(),
        "up" => "\u{2191}".into(),
        "down" => "\u{2193}".into(),
        "left" => "\u{2190}".into(),
        "right" => "\u{2192}".into(),
        "space" => "space".into(),
        key => key.to_uppercase(),
    }
}

/// A quiet text control: `CONTROL_H`, the grid's type, `W_BODY` `INK_STRONG`;
/// hover `paint::HOVER`, press `paint::PRESS`. A button is read like any
/// row, so it never takes the heading weight.
pub fn ghost_button(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    faded_button(
        id,
        rgba(theme::TRANSPARENT).into(),
        theme::paint::HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(theme::INK_STRONG).into(),
        cx,
    )
    .h(px(theme::CONTROL_H))
    .px(px(theme::CONTROL_PAD_X))
    .child(
        text_ui()
            .font_weight(theme::W_BODY)
            .text_color(rgb(theme::INK_STRONG))
            .child(label.into()),
    )
}

/// A secondary button (SPEC §5.2, §5.3): `CONTROL_H` at `R_CONTROL` on
/// `paint::RAISED`, stepping to `paint::SELECTED` under the pointer and
/// `paint::PRESS` pressed; its label `FS_LABEL` at `W_LABEL` in
/// `INK_STRONG`, led by a `CONTROL_CHIP_ICON` glyph when it has one (then
/// padded 10 / 12, the icon `CONTROL_CHIP_GAP` before the label).
pub fn secondary_button(
    id: impl Into<ElementId>,
    glyph: Option<&'static str>,
    label: impl Into<SharedString>,
    cx: &App,
) -> Button {
    faded_button(
        id,
        theme::paint::RAISED.into(),
        theme::paint::SELECTED.into(),
        theme::paint::PRESS.into(),
        rgb(theme::INK_STRONG).into(),
        cx,
    )
    .h(px(theme::CONTROL_H))
    .rounded(px(theme::R_CONTROL))
    .pl(px(if glyph.is_some() {
        theme::CONTROL_CHIP_PAD_X
    } else {
        theme::CONTROL_PAD_X
    }))
    .pr(px(theme::CONTROL_PAD_X))
    .child(
        text_ui()
            .flex()
            .items_center()
            .gap(px(theme::CONTROL_CHIP_GAP))
            .text_size(px(theme::FS_LABEL))
            .font_weight(theme::W_LABEL)
            .text_color(rgb(theme::INK_STRONG))
            .children(
                glyph.map(|glyph| icons::icon(glyph, theme::CONTROL_CHIP_ICON, theme::INK_STRONG)),
            )
            .child(label.into()),
    )
}

// ------------------------------------------------------------------ menus

/// One menu row's content.
#[derive(Clone, Debug, Default)]
pub struct MenuItem {
    pub label: SharedString,
    /// Fuzzy-match runs in `label`, drawn in `INK_STRONG` at `W_LABEL`.
    pub matched: Vec<Range<usize>>,
    /// An aligned name column's width, when the rows share one.
    pub label_w: Option<f32>,
    /// A 12px leading mark and its ink.
    pub leading: Option<(&'static str, u32)>,
    /// A trailing detail: Ferrite's description of the row (`INK_SUBTLE` in
    /// every state), or — when `mono` — machine text such as an `@` path,
    /// `INK_SUBTLE`, cut at its head so the useful tail survives.
    pub detail: Option<SharedString>,
    /// `detail` is machine text (rule 2.1.1): drawn in the code face.
    pub mono: bool,
    /// The key that does the same thing, faint at the right edge.
    pub shortcut: Option<SharedString>,
    /// A one-line description under the label (`INK_SUBTLE`, the meta size):
    /// the row grows to `FLOAT_ROW_NOTE_H` (a mode's "Always ask before
    /// making changes").
    pub note: Option<SharedString>,
    /// A small outlined tag after the label (`Recommended`, `Default`).
    pub badge: Option<SharedString>,
    /// The same outlined tag at the row's right edge, after its detail:
    /// where a `/` command comes from (`Skill`, `Plugin`), so an aligned
    /// description column stays aligned.
    pub tag: Option<SharedString>,
    pub checked: bool,
    /// Arms before it runs: at rest it reads like any row; armed, its label
    /// turns `BLOCKED` and asks for the second press.
    pub destructive: bool,
    pub disabled: bool,
    /// Why a disabled row is disabled, in its tooltip: a row stays one
    /// line, as Claude's do.
    pub why: Option<SharedString>,
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
    pub fn note(mut self, text: impl Into<SharedString>) -> Self {
        self.note = Some(text.into());
        self
    }
    pub fn badge(mut self, text: impl Into<SharedString>) -> Self {
        self.badge = Some(text.into());
        self
    }
    pub fn tag(mut self, text: impl Into<SharedString>) -> Self {
        self.tag = Some(text.into());
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
    /// Disabled, saying `why` in the row's tooltip.
    pub fn disabled_because(mut self, why: impl Into<SharedString>) -> Self {
        self.disabled = true;
        self.why = Some(why.into());
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

/// The row state table, as data: rest, cursor, disabled, armed — every
/// menu row's inks (`menu::row_face`, `menu_row_content`). The name is
/// content (`INK_STRONG`) in every live state, as Claude's menus; the
/// description and key are meta (`INK_SUBTLE`), lifted to `INK_STRONG` on
/// the filled cursor (or armed) row, where meta falls under its floor. A
/// destructive row's label is `BLOCKED` in every live state, as Claude's
/// red `Delete`; armed, it also holds the cursor's `paint::FLOAT_SEL`
/// ground — colour on the word, never a wash (rule 2.2.4).
pub fn row_inks(item: &MenuItem, cursor: bool, armed: bool) -> RowInks {
    if item.disabled {
        return RowInks {
            label: theme::INK_SUBTLE,
            detail: theme::INK_SUBTLE,
            shortcut: theme::INK_SUBTLE,
            ground: None,
        };
    }
    let filled = cursor || armed;
    let label = if armed || item.destructive {
        theme::BLOCKED
    } else {
        theme::INK_STRONG
    };
    let meta = if filled {
        theme::INK_STRONG
    } else {
        theme::INK_SUBTLE
    };
    RowInks {
        label,
        detail: meta,
        shortcut: meta,
        ground: filled.then_some(theme::paint::FLOAT_SEL.rgba()),
    }
}

/// Fuzzy-match runs as highlights (the prototype's `.m`): `INK_STRONG` at
/// `W_LABEL`, in every float that filters — the palette, the `/` and `@`
/// menus. One face, so the heavier run never moves a column. A disabled
/// row paints none.
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
                    color: Some(rgb(theme::INK_STRONG).into()),
                    font_weight: Some(theme::W_LABEL),
                    ..Default::default()
                },
            )
        })
        .collect()
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
        .h(px(theme::FLOAT_ROW_H))
        .px(px(theme::MENU_ROW_PAD_X))
        .rounded(px(theme::R_CONTROL))
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
                    // An aligned column, in the row's own face.
                    label.w(px(width)).flex_shrink_0()
                })
                .child(gpui::StyledText::new(label).with_highlights(highlights)),
        )
        .when_some(item.detail.clone(), |row, detail| {
            let cell = div().flex_1().min_w_0().truncate();
            let cell = if item.mono {
                cell.font_family(theme::FONT_CODE)
                    .text_size(px(theme::FS_UI))
                    .text_color(rgb(theme::INK_SUBTLE))
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
                    .text_size(px(theme::FS_UI))
                    .text_color(rgb(inks.shortcut))
                    .child(key),
            )
        })
        .when(item.checked, |row| {
            row.child(glyph("\u{2713}", theme::MENU_CHECK_INK))
        })
}

/// A path as the operator reads it: their home spelled `~`.
pub fn home_relative(path: &std::path::Path) -> String {
    let shown = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && shown.starts_with(&home) => {
            format!("~{}", &shown[home.len()..])
        }
        _ => shown,
    }
}

/// A fold's chevron, every surface's (the nav's sections and Groups, the
/// panel's file blocks, the usage card's folds): `chevron-right` while
/// folded, a quarter turn down while open. The turn eases over
/// `motion::CHEVRON` (Claude's 60ms) when the fold flips under it, keyed
/// by `id`; first paint lands (`motion::settled`).
pub fn fold_chevron(id: impl Into<ElementId>, open: bool, size: f32, ink: u32) -> impl IntoElement {
    motion::settled(id, open, motion::CHEVRON, move |turn| {
        chevron_at(turn, size, ink)
    })
}

/// `chevron-right` `turn` of a quarter turn down (0 folded, 1 open).
pub fn chevron_at(turn: f32, size: f32, ink: u32) -> gpui::Svg {
    icons::icon(icons::CHEVRON_RIGHT, size, ink).with_transformation(gpui::Transformation::rotate(
        gpui::radians(std::f32::consts::FRAC_PI_2 * turn),
    ))
}

/// A menu row. The only place a menu row takes its pointer role: the raised
/// hover and press faces, or the carried face on the cursor row; an armed or
/// disabled row takes none. Callers add selectors and handlers only.
#[cfg(test)]
pub fn menu_row(
    id: impl Into<ElementId>,
    item: &MenuItem,
    cursor: bool,
    armed: bool,
) -> gpui::Stateful<Div> {
    use crate::pointer::{Pointer, PointerPressed};
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

/// A menu section title: the grid's type, `W_LABEL` `INK_SUBTLE`, an optional
/// leading mark and an optional note after it. Its mark and title share the
/// rows' leading edge. A section that follows rows is set apart from them
/// by `.mt(MENU_GROUP_GAP)` (space), never a rule.
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
        .h(px(theme::FLOAT_SECTION_H))
        .px(px(theme::MENU_ROW_PAD_X))
        .pb(px(theme::SPACE_1))
        .cursor_default()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_2))
                .h(px(theme::LH_UI))
                .min_w_0()
                .when_some(leading, |line, (path, ink)| {
                    line.child(icons::icon(path, theme::STATUS_LOGO, ink))
                })
                .child(div().font_weight(theme::W_LABEL).child(title.into()))
                .children(note),
        )
}

/// Reports `element`'s laid-out bounds, unrounded, to `record` in
/// prepaint, through an absolute box pinned to all four edges of its
/// padding box, so padding never offsets it. A border is outside that box:
/// an edged caller adds it back. What a summoned surface measures its
/// trigger and limits by.
pub fn on_bounds<E: ParentElement>(
    element: E,
    record: impl FnOnce(gpui::Bounds<gpui::Pixels>, &mut Window, &mut App) + 'static,
) -> E {
    element.child(Measured {
        record: Some(Box::new(record)),
    })
}

type Record = Box<dyn FnOnce(gpui::Bounds<gpui::Pixels>, &mut Window, &mut App)>;

/// An absolute box pinned to its parent's padding box that reports the
/// box's unrounded bounds in prepaint: where the browser's
/// `getBoundingClientRect` would put it, before the pixel rounding of what
/// is painted (vendor/gpui-pre, "Pixel snapping").
struct Measured {
    record: Option<Record>,
}

impl IntoElement for Measured {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for Measured {
    type RequestLayoutState = gpui::LayoutId;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, gpui::LayoutId) {
        let mut style = gpui::Style::default();
        style.position = gpui::Position::Absolute;
        style.inset.top = px(0.).into();
        style.inset.right = px(0.).into();
        style.inset.bottom = px(0.).into();
        style.inset.left = px(0.).into();
        let layout_id = window.request_layout(style, None, cx);
        (layout_id, layout_id)
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        layout_id: &mut gpui::LayoutId,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(record) = self.record.take() {
            let bounds = window.unsnapped_layout_bounds(*layout_id);
            record(bounds, window, cx);
        }
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut gpui::LayoutId,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

/// One row of a picker (`menu::ChoiceMenu`), opened by a chip or a slash
/// command alike; it draws in the one menu grammar (`MenuItem`).
#[derive(Clone, Default)]
pub struct Choice {
    pub label: SharedString,
    /// A section's mark, or a row's own when the menu has no sections.
    pub icon: Option<(&'static str, u32)>,
    /// The strong check: the standing choice.
    pub checked: bool,
    pub disabled: bool,
    /// A section title: inert, skipped by the arrows, carrying `icon`.
    pub section: bool,
    /// A muted mono tag after the label (a section tag, a path).
    pub detail: Option<SharedString>,
    /// Why a disabled row is dead for now (`Available when this turn
    /// finishes`): its tooltip (SPEC §5.5).
    pub why: Option<SharedString>,
    /// The row's one-line description under its label (`MenuItem::note`).
    pub note: Option<SharedString>,
    /// A tag after the label (`MenuItem::badge`).
    pub badge: Option<SharedString>,
    /// The key that picks it, at the right edge (a mode's `1`…`5`).
    pub shortcut: Option<SharedString>,
}

impl Choice {
    /// The row's content in the menu grammar.
    pub(crate) fn item(&self, marked: bool) -> MenuItem {
        let mut item = MenuItem::new(self.label.clone())
            .checked(self.checked)
            .disabled(self.disabled);
        if let Some(detail) = &self.detail {
            item = item.detail(detail.clone());
        }
        item.note = self.note.clone();
        item.badge = self.badge.clone();
        item.shortcut = self.shortcut.clone();
        if let (true, Some((path, ink))) = (marked, self.icon) {
            item = item.leading(path, ink);
        }
        item
    }
}

/// The limits a summoned surface rests within (rule 2.4.3): `floor` is the
/// edge a surface opening upward rests `FLOAT_OFFSET` above so that edge
/// stays whole — the bar's top for a status-line trigger, a Pane's Composer
/// for its own; `limit_right` is the right edge it never crosses — the
/// window's margin, or the Pane's inner edge (`PANE_PAD_X` in).
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
    /// inside `limit_right`.
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
}

/// A menu grammar row inside a kit `PopupMenuItem`: the kit item already
/// insets its content by the row's own inline padding, so the row takes it
/// back and spans the item edge to edge. The kit draws the hover and cursor
/// face (`tokens.strong` = `SELECTION`) on the item itself.
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
    scrollbar_layer().child(scrollbar_bar(id, scroll))
}

/// The overlay the bar hangs in: over the whole scroller, taking no
/// layout. A scroller with a padding of its own to lay the bar in (the
/// nav's) moves its edge out.
pub fn scrollbar_layer() -> gpui::Div {
    div().absolute().top_0().left_0().right_0().bottom_0()
}

/// The vertical bar itself, in hover mode (see `scrollbar`).
pub fn scrollbar_bar(
    id: impl Into<ElementId>,
    scroll: &(impl gpui::base::ScrollbarHandle + Clone),
) -> crate::scrollbar::Scrollbar {
    crate::scrollbar::Scrollbar::vertical(scroll)
        .id(id)
        .scrollbar_show(gpui::component::scroll::ScrollbarMode::Hover)
}

/// A CI check's state, one vocabulary for every surface that lists checks
/// (the Pane head's checks card), so a state reads the same a click apart. Colour is state (rule 6):
/// a failure is `BLOCKED`; a run still going the working mark, its orbs
/// white; a pass is done, a shape (`check-circle`) in `INK_SUBTLE`, never
/// the green a running one wears; a queued run a hollow ring and a skipped
/// one the ring crossed by a dash, both in `TEXT_FAINT` — they claim
/// nothing. Only a failure colours its word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckMark {
    Failed,
    Running,
    Queued,
    Passed,
    Skipped,
}

impl CheckMark {
    /// The mark's ink.
    pub fn ink(self) -> u32 {
        match self {
            CheckMark::Failed => theme::BLOCKED,
            CheckMark::Running => theme::RUNNING,
            CheckMark::Passed => theme::INK_SUBTLE,
            CheckMark::Queued | CheckMark::Skipped => theme::TEXT_FAINT,
        }
    }

    /// The state word's ink: only a failure is coloured.
    pub fn word_ink(self) -> u32 {
        match self {
            CheckMark::Failed => theme::BLOCKED,
            _ => theme::INK_SUBTLE,
        }
    }

    /// The mark in its `CHECK_MARK` box.
    pub fn element(self) -> AnyElement {
        let ink = self.ink();
        let ring = || {
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(theme::CHECK_MARK_RING))
                .rounded_full()
                .border_1()
                .border_color(rgb(ink))
        };
        let drawn = match self {
            CheckMark::Failed => {
                icons::icon(icons::X_CIRCLE, theme::CHECK_MARK, ink).into_any_element()
            }
            CheckMark::Passed => {
                icons::icon(icons::CHECK_CIRCLE, theme::CHECK_MARK, ink).into_any_element()
            }
            CheckMark::Running => working_mark(theme::CHECK_MARK, theme::INK_STRONG),
            CheckMark::Queued => ring().into_any_element(),
            CheckMark::Skipped => ring()
                .child(div().w(px(theme::CHECK_SKIP_DASH)).h(px(1.)).bg(rgb(ink)))
                .into_any_element(),
        };
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(theme::CHECK_MARK))
            .child(drawn)
            .into_any_element()
    }
}

/// The one copy button (a message's `Copy`, a changed file's `Copy path`,
/// a preview's): an icon door that writes `text` to the clipboard and
/// says so, its `copy` glyph turning to a `check` for `COPIED_HOLD_MS`,
/// then back. The check alone says it: gpui hides a tooltip on the press
/// and builds the next one once, so a swapped tooltip would show late and
/// outlive the hold. The door is `size` square at
/// `radius` holding a `glyph`-sized icon in `INK_SUBTLE`; its hover is the
/// raised blend, or, on a ground already `RAISED` (`on_raised`), a step up
/// to `SELECTED`. It keeps its own moment of having copied (keyed by `id`),
/// so a caller holds no state for it. A press on it is its own: it never
/// reaches the row under it.
#[derive(IntoElement)]
pub struct CopyButton {
    id: ElementId,
    text: SharedString,
    tooltip: SharedString,
    size: f32,
    glyph: f32,
    radius: f32,
    on_raised: bool,
}

/// A [`CopyButton`] for `text`, `Copy` its tooltip at rest.
pub fn copy_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> CopyButton {
    CopyButton {
        id: id.into(),
        text: text.into(),
        tooltip: "Copy".into(),
        size: theme::MESSAGE_ACTION,
        glyph: theme::MESSAGE_ACTION_GLYPH,
        radius: theme::R_CONTROL,
        on_raised: false,
    }
}

impl CopyButton {
    /// The words of its tooltip at rest (`Copy path`), drawn as the
    /// measured tooltip (`menu::tooltip`).
    pub fn tooltip_text(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = tooltip.into();
        self
    }

    /// The door's box, its glyph and its corner radius.
    pub fn door(mut self, size: f32, glyph: f32, radius: f32) -> Self {
        self.size = size;
        self.glyph = glyph;
        self.radius = radius;
        self
    }

    /// The door stands on a `RAISED` ground: its hover steps up to
    /// `SELECTED`.
    pub fn on_raised(mut self) -> Self {
        self.on_raised = true;
        self
    }
}

impl gpui::RenderOnce for CopyButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use crate::pointer::{Pointer, PointerPressed};
        let copied = window.use_keyed_state(
            SharedString::from(format!("copy-button-{:?}", self.id)),
            cx,
            |_, _| None::<std::time::Instant>,
        );
        let shown = copied.read(cx).is_some();
        let key = crate::pointer::hover_key(&self.id);
        let text = self.text;
        let door = div()
            .id(self.id)
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(self.size))
            .rounded(px(self.radius));
        let door = if self.on_raised {
            door.cursor_pointer()
                .bg(motion::hover_blend(
                    &key,
                    theme::paint::RAISED.into(),
                    theme::paint::SELECTED.into(),
                ))
                .on_hover(motion::hover_listener(key))
        } else {
            door.hover_raised(key)
        };
        door.press_raised()
            .tooltip(crate::menu::tooltip(self.tooltip))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_string()));
                // Each copy holds its own moment: a second one restarts
                // the hold rather than being cut short by the first's end.
                let at = std::time::Instant::now();
                copied.update(cx, |copied, cx| {
                    *copied = Some(at);
                    cx.notify();
                });
                let copied = copied.downgrade();
                let hold = Duration::from_millis(theme::COPIED_HOLD_MS);
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(hold).await;
                    let _ = copied.update(cx, |copied, cx| {
                        if *copied == Some(at) {
                            *copied = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            })
            .child(icons::icon(
                if shown { icons::CHECK } else { icons::COPY },
                self.glyph,
                theme::INK_SUBTLE,
            ))
    }
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

    /// The line-end fade runs `LINE_FADE` wide at the right of its box,
    /// from clear to the ground it is given.
    #[test]
    fn a_line_fade_runs_clear_to_its_ground() {
        let fill: Hsla = rgb(theme::SIDEBAR).into();
        let style = line_fade(fill).style().clone();
        assert_eq!(style.size.width, Some(px(theme::LINE_FADE).into()));
        assert_eq!(
            style.background,
            Some(gpui::Fill::Color(gpui::linear_gradient(
                90.,
                gpui::linear_color_stop(Hsla { a: 0., ..fill }, 0.),
                gpui::linear_color_stop(fill, 1.),
            )))
        );
    }

    /// Theme rule 2: square and flat inside. Only a float (and a sheet,
    /// which floats) casts; no rung, keycap, well or knob is lit.
    #[test]
    fn only_a_float_casts_and_nothing_is_lit() {
        use Elevation::*;
        for rung in [Pane, Lifted] {
            assert!(elevation(rung).is_empty(), "{rung:?} is flat");
        }
        for rung in [Float, Sheet] {
            assert_eq!(elevation(rung), float_shadow(), "{rung:?} floats");
        }
        assert!(shadow_faded(float_shadow(), 0.5)[0].color.a < float_shadow()[0].color.a);
    }

    #[test]
    fn a_floating_surface_wears_the_float_ground_the_strong_line_and_the_shadow() {
        let mut surface = floating_surface();
        let style = surface.style();
        assert_eq!(style.background, Some(Fill::from(theme::paint::FLOAT)));
        assert_eq!(style.border_color, Some(theme::paint::FLOAT_EDGE.into()));
        // Floats are opaque (FL-15).
        assert!(theme::paint::FLOAT.is_opaque() && theme::paint::FLOAT_SEL.is_opaque());
        assert_eq!(style.corner_radii.top_left, Some(px(theme::R_FLOAT).into()));
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
                theme::INK_STRONG,
            ),
            (
                text_meta(),
                theme::FONT_UI,
                theme::FS_UI,
                theme::LH_UI,
                theme::INK_SUBTLE,
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
        assert_eq!(shadow.color, rgba(theme::FOCUS_RING).into());
        assert_eq!(shadow.spread_radius, px(theme::FOCUS_RING_W));
        assert_eq!(theme::FOCUS_RING_W, 1.);
        let mut ringed = focused(div(), true);
        assert_eq!(ringed.style().box_shadow, focus.box_shadow);
        let mut plain = focused(div(), false);
        assert_eq!(plain.style().box_shadow, None);
        let primary = primary_focus(StyleRefinement::default());
        let shadow = &primary.box_shadow.expect("an inset outline")[0];
        assert_eq!(shadow.color, rgb(theme::INK_STRONG).into());
        assert_eq!(primary_face(false), theme::PRIMARY);
        assert_eq!(primary_ink(false), theme::ON_PRIMARY);
        assert_eq!(primary_face(true), theme::SELECTED);
        assert_eq!(primary_ink(true), theme::INK_SUBTLE);
    }

    #[test]
    fn marks_hold_their_boxes() {
        // The dots are the face's own glyphs, typed in their ink.
        let mut dot = status_dot(theme::RUNNING);
        assert_eq!(
            dot.style().text.color,
            Some(rgb(theme::RUNNING).into()),
            "● in its ink"
        );
        assert_eq!(dot.style().background, None, "typed, not painted");
        let mut ring = status_ring(theme::TEXT_FAINT);
        assert_eq!(ring.style().text.color, Some(rgb(theme::TEXT_FAINT).into()));
        let mut gutter = gutter(div(), theme::LH_UI);
        assert_eq!(gutter.style().size.width, Some(px(theme::GUTTER_W).into()));
    }

    #[test]
    fn menu_rows_follow_the_state_table() {
        let rest = MenuItem::new("Rename");
        let fill = theme::paint::FLOAT_SEL.rgba();
        assert_eq!(
            row_inks(&rest, false, false),
            RowInks {
                label: theme::INK_STRONG,
                detail: theme::INK_SUBTLE,
                shortcut: theme::INK_SUBTLE,
                ground: None
            }
        );
        // The cursor row lifts its meta to content on its fill.
        let cursor = row_inks(&rest, true, false);
        assert_eq!(
            cursor,
            RowInks {
                label: theme::INK_STRONG,
                detail: theme::INK_STRONG,
                shortcut: theme::INK_STRONG,
                ground: Some(fill)
            }
        );
        let delete = MenuItem::new("Delete thread").destructive();
        assert_eq!(row_inks(&delete, false, false).label, theme::BLOCKED);
        assert_eq!(row_inks(&delete, true, false).label, theme::BLOCKED);
        let armed = row_inks(&delete, false, true);
        assert_eq!((armed.label, armed.ground), (theme::BLOCKED, Some(fill)));
        let dead = MenuItem::new("Reveal").disabled(true);
        let inks = row_inks(&dead, true, false);
        assert_eq!((inks.label, inks.ground), (theme::INK_SUBTLE, None));
        // TEXT_FAINT is never text, in any state.
        for inks in [row_inks(&rest, false, false), cursor, armed, inks] {
            assert!(![inks.label, inks.detail, inks.shortcut].contains(&theme::TEXT_FAINT));
        }
        assert_eq!(MenuItem::new("x").shortcut("").shortcut, None);
        assert!(match_highlights(std::slice::from_ref(&(0..2)), true).is_empty());
        let lit = match_highlights(std::slice::from_ref(&(0..2)), false);
        assert_eq!(lit[0].1.font_weight, Some(theme::W_LABEL));
        assert_eq!(lit[0].1.color, Some(rgb(theme::INK_STRONG).into()));
    }

    #[test]
    fn a_menu_row_is_one_row_high_and_only_live_rows_point() {
        let mut row = menu_row("r", &MenuItem::new("Rename"), false, false);
        assert_eq!(row.style().size.height, Some(px(theme::FLOAT_ROW_H).into()));
        assert_eq!(
            row.style().mouse_cursor,
            Some(gpui::CursorStyle::PointingHand)
        );
        let mut dead = menu_row("d", &MenuItem::new("Reveal").disabled(true), false, false);
        assert_eq!(dead.style().mouse_cursor, None);
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
            theme::INK_SUBTLE
        );
        // A menu row is neutral whatever it names: `Manual`'s amber is
        // its footer chip's alone (Claude's).
        let manual = Choice {
            label: "Manual".into(),
            ..Default::default()
        };
        assert_eq!(
            row_inks(&manual.item(false), false, false).label,
            theme::INK_STRONG
        );
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
    }

    /// The shimmer's crest travels left to right across the run, peaks at
    /// the crest colour, and leaves the rest of the run in its base colour.
    #[test]
    fn the_shimmer_crest_sweeps_left_to_right() {
        // Before and after the sweep: nothing is lit.
        for x in [0.0, 0.5, 1.0] {
            assert_eq!(shimmer_lift(x, 0.0), 0.0);
        }
        // Mid-sweep the crest sits at the run's middle, symmetric.
        assert_eq!(shimmer_lift(0.5, 0.5), 1.0);
        assert!((shimmer_lift(0.3, 0.5) - shimmer_lift(0.7, 0.5)).abs() < 1e-5);
        // It travels left to right.
        assert!(shimmer_lift(0.2, 0.35) > shimmer_lift(0.2, 0.65));
        assert!(shimmer_lift(0.8, 0.35) < shimmer_lift(0.8, 0.65));
        // Claude's clay crests at the prototype's `#ffe1d3`, lighter than it.
        let base: Hsla = rgb(theme::PROVIDER_CLAUDE).into();
        let crest = shimmer_crest(theme::PROVIDER_CLAUDE);
        assert_eq!(crest, rgb(theme::SHIMMER_CREST_CLAUDE).into());
        assert!(crest.l > base.l);
    }

    /// The caret's hard blink: shown for the first half of a 1s turn,
    /// hidden for the second, nothing between.
    #[test]
    fn the_caret_blinks_hard() {
        assert_eq!(theme::MOTION_CARET_BLINK_MS, 1_000);
        assert_eq!(caret_blink(0.0), 1.0);
        assert_eq!(caret_blink(0.49), 1.0);
        assert_eq!(caret_blink(0.5), 0.0);
        assert_eq!(caret_blink(0.9999), 0.0);
        assert_eq!(caret_blink(1.0), 1.0, "a period wraps");
    }

    /// The pulse clock draws the caret on `CARET_STEPS`: inside each step
    /// the blink holds exactly one value.
    #[test]
    fn the_caret_holds_still_inside_each_declared_step() {
        let motion::Motion::Frames(steps) = CARET_STEPS else {
            unreachable!("the blink moves in steps")
        };
        for step in 0..100_000 {
            let p = step as f32 / 100_000.0;
            let first = (frame_at(p, steps) as f32) / steps as f32;
            assert_eq!(
                caret_blink(p),
                caret_blink(first),
                "moved at {p} inside a step"
            );
        }
    }

    /// In tests the kit rests (reduced motion): each loop draws its still
    /// state and leases nothing, so a board of working marks schedules no frame.
    #[gpui::test]
    fn working_marks_and_the_shimmer_hold_still_at_rest(cx: &mut gpui::TestAppContext) {
        struct Board;
        impl gpui::Render for Board {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                div()
                    .child(working_mark(theme::CHECK_MARK, theme::RUNNING))
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

    /// Lit, the shimmer rides the display's frames, as the orbs do: it
    /// asks for the next frame and declares nothing on the ~30fps pulse
    /// clock, which stays parked.
    #[gpui::test]
    fn a_lit_shimmer_leases_display_frames_not_the_pulse_clock(cx: &mut gpui::TestAppContext) {
        motion::testing::drive();
        struct Line;
        impl gpui::Render for Line {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                div().child(shimmer("Working", theme::PROVIDER_CLAUDE))
            }
        }
        let window = cx.add_window(|_, _| Line);
        cx.run_until_parked();
        window
            .update(cx, |_, window, cx| {
                assert!(motion::testing::frame_leased(window, cx), "one frame asked");
                assert!(motion::pulse_parked(cx), "no 30fps tick declared");
            })
            .unwrap();
    }

    /// Lit, the shimmer paints the line its run laid out as: in a box
    /// narrower than its words, cut with `…` as the still run is, never
    /// the whole run clipped mid-glyph.
    #[gpui::test]
    fn a_narrow_shimmer_is_cut_with_an_ellipsis(cx: &mut gpui::TestAppContext) {
        motion::testing::drive();
        struct Narrow;
        impl gpui::Render for Narrow {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                div().flex().w(px(80.)).child(shimmer(
                    "Checking the supplied arguments",
                    theme::PROVIDER_CLAUDE,
                ))
            }
        }
        cx.add_window(|_, _| Narrow);
        cx.run_until_parked();
        let painted = loops::SHIMMER_PAINTED.with(|painted| painted.borrow().clone());
        let painted = painted.expect("the shimmer painted lit");
        assert!(painted.ends_with('…'), "cut with an ellipsis: {painted:?}");
        assert!(painted.len() < "Checking the supplied arguments".len());
    }
}
