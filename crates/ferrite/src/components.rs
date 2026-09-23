//! Ferrite's shared primitives: the one way render code opens a text run,
//! lays a plane or a floating surface, draws a status mark, a keycap or a
//! chip, and builds a control or a menu row. Longbridge owns the control
//! mechanics; `theme.rs` is the only token source.
//!
//! Frozen after the foundation (F3): a package that needs something new
//! builds it privately and asks for a promotion. A bug fix comes with a
//! failing test first. Styles are asserted as data in `tests` below.
// The work packages adopt these primitives; stabilization removes this allow.
#![allow(dead_code)]

use std::ops::Range;
use std::time::Duration;

use gpui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui::component::{FocusableExt, Sizable};
use gpui::prelude::*;
use gpui::{
    div, point, pulsating_between, px, rgb, rgba, Animation, AnimationExt, AnyElement, App,
    BoxShadow, Div, ElementId, FontFeatures, HighlightStyle, SharedString, Stateful,
    StyleRefinement,
};

use crate::icons;
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

/// Keyboard focus, the one recipe: a 2px inset `FOCUS_RING` outline. It
/// survives hover's border/background refinements and stays inside clipped
/// forms without taking any layout space.
pub fn control_focus(style: StyleRefinement) -> StyleRefinement {
    focus_outline(style, theme::FOCUS_RING)
}

fn focus_outline(style: StyleRefinement, ink: u32) -> StyleRefinement {
    style.shadow(vec![BoxShadow {
        inset: true,
        color: rgb(ink).into(),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
    }])
}

/// Form actions need an opaque hover face on the modal's raised ground.
/// Use the toolkit's variant API: its renderer owns hover/press handlers.
pub fn form_button(id: impl Into<ElementId>, cx: &App) -> Button {
    button(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(theme::TEXT).into())
                .hover(rgb(theme::FILL).into())
                .active(rgb(theme::FILL_HOVER).into()),
        )
        .tab_stop(true)
}

/// The completing action: steel `ACCENT_STRONG` with white ink, hovering to
/// `PRIMARY_HOVER` and pressing to `PRIMARY_ACTIVE`; disabled is `FILL` with
/// `TEXT_MUTED` ink. On the filled face the focus outline is `TEXT_STRONG`.
pub fn primary_button(id: impl Into<ElementId>, disabled: bool, cx: &App) -> Button {
    use gpui::component::Disableable;
    form_button(id, cx)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(primary_ink(disabled)).into())
                .hover(rgb(theme::PRIMARY_HOVER).into())
                .active(rgb(theme::PRIMARY_ACTIVE).into()),
        )
        .bg(rgb(primary_face(disabled)))
        .text_color(rgb(primary_ink(disabled)))
        .focus_visible(|style| focus_outline(style, theme::TEXT_STRONG))
        .disabled(disabled)
        .when(disabled, |button| button.cursor_default())
}

fn primary_face(disabled: bool) -> u32 {
    if disabled {
        theme::FILL
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

pub fn label(text: impl Into<SharedString>, ink: u32) -> impl IntoElement {
    div()
        .text_size(px(theme::FS_SM))
        .line_height(gpui::px(theme::LH_META))
        .text_color(rgb(ink))
        .child(text.into())
}

/// Forms use the body size so values and actions read at the same scale as
/// their labels. Dense pane chrome continues to use `label`.
pub fn form_label(text: impl Into<SharedString>, ink: u32) -> impl IntoElement {
    div()
        .text_size(px(theme::FS_UI))
        .line_height(gpui::px(theme::LH_UI))
        .text_color(rgb(ink))
        .child(text.into())
}

// ------------------------------------------------------------------- type

/// A mono UI line: `FONT_MONO` · `FS_UI` on `LH_UI` · `TEXT`.
pub fn text_ui() -> Div {
    div()
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(theme::TEXT))
}

/// Metadata: `FONT_MONO` · `FS_SM` on `LH_META` · `TEXT_MUTED`.
pub fn text_meta() -> Div {
    div()
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_MUTED))
}

/// Prose: `FONT_PROSE` · `FS_PROSE` on `LH_PROSE` · `TEXT`.
pub fn text_prose() -> Div {
    div()
        .font_family(theme::FONT_PROSE)
        .text_size(px(theme::FS_PROSE))
        .line_height(px(theme::LH_PROSE))
        .text_color(rgb(theme::TEXT))
}

/// A group's title inside a surface: mono `FS_SM` `W_LABEL` `TEXT_MUTED`,
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

/// An elapsed time in compact units: `0.3s`, `12s`, `1m04s`.
pub fn duration_label(elapsed: Duration) -> SharedString {
    let secs = elapsed.as_secs_f64().max(0.1);
    if secs < 10.0 {
        SharedString::from(format!("{secs:.1}s"))
    } else if secs < 60.0 {
        SharedString::from(format!("{}s", secs as u64))
    } else {
        let whole = secs as u64;
        SharedString::from(format!("{}m{:02}s", whole / 60, whole % 60))
    }
}

// ------------------------------------------------- planes and elevation

/// An in-flow raised block (Composer, code, cards): `RAISED`, `R_BLOCK`, no
/// edge and no shadow.
pub fn raised() -> Div {
    div().bg(rgb(theme::RAISED)).rounded(px(theme::R_BLOCK))
}

/// A raised block with a 1px edge that is always in layout, so a state change
/// recolours the edge and never shifts what is inside.
pub fn raised_edged(edge: u32) -> Div {
    raised().border_1().border_color(rgba(edge))
}

/// The one rule weight, horizontal.
pub fn hairline() -> Div {
    div()
        .h(px(1.))
        .w_full()
        .flex_shrink_0()
        .bg(rgba(theme::HAIRLINE))
}

/// The one rule weight, vertical.
pub fn vhairline() -> Div {
    div()
        .w(px(1.))
        .h_full()
        .flex_shrink_0()
        .bg(rgba(theme::HAIRLINE))
}

/// The only shadow in the app, for floating surfaces: a far soft layer and a
/// near contact layer.
pub fn float_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            inset: false,
            color: rgba(theme::SHADOW_FAR).into(),
            offset: point(px(0.), px(theme::SHADOW_FAR_Y)),
            blur_radius: px(theme::SHADOW_FAR_BLUR),
            spread_radius: px(theme::SHADOW_FAR_SPREAD),
        },
        BoxShadow {
            inset: false,
            color: rgba(theme::SHADOW_NEAR).into(),
            offset: point(px(0.), px(theme::SHADOW_NEAR_Y)),
            blur_radius: px(theme::SHADOW_NEAR_BLUR),
            spread_radius: px(0.),
        },
    ]
}

/// A floating surface (menu, popover, card): `MENU` ground, a
/// `HAIRLINE_STRONG` edge, `R_BLOCK`, the float shadow, `FLOAT_PAD` inside,
/// mono UI type. It occludes what it covers and owns its cursor. The caller
/// states its width and position.
pub fn floating_surface() -> Div {
    text_ui()
        .cursor_default()
        .occlude()
        .flex()
        .flex_col()
        .p(px(theme::FLOAT_PAD))
        .rounded(px(theme::R_BLOCK))
        .bg(rgb(theme::MENU))
        .border_1()
        .border_color(rgba(theme::HAIRLINE_STRONG))
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

/// A status dot with a halo that breathes behind it on the `STATUS_PULSE_MS`
/// loop; still when the operator asked for reduced motion. The halo is
/// absolute and the box is a fixed `STATUS_DOT`, so nothing around it moves.
pub fn pulsing_dot(
    id: impl Into<ElementId>,
    ink: u32,
    halo: u32,
    reduce_motion: bool,
) -> AnyElement {
    let ring = div()
        .absolute()
        .left(px(-theme::STATUS_HALO_INSET))
        .top(px(-theme::STATUS_HALO_INSET))
        .size(px(theme::STATUS_DOT + 2. * theme::STATUS_HALO_INSET))
        .rounded_full()
        .bg(rgba(halo));
    let ring = if reduce_motion {
        ring.opacity(theme::PULSE_MIN).into_any_element()
    } else {
        ring.with_animation(
            id,
            Animation::new(Duration::from_millis(theme::STATUS_PULSE_MS))
                .repeat()
                .with_easing(pulsating_between(theme::PULSE_MIN, 1.0)),
            |ring, delta| ring.opacity(delta),
        )
        .into_any_element()
    };
    div()
        .relative()
        .flex_shrink_0()
        .size(px(theme::STATUS_DOT))
        .child(ring)
        .child(status_dot(ink))
        .into_any_element()
}

/// The one keycap: `KBD_H`, at least square, `RAISED_2`, mono `FS_SM`
/// `TEXT_2`, centred.
pub fn kbd(key: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(theme::KBD_H))
        .min_w(px(theme::KBD_H))
        .px(px(theme::KBD_PAD_X))
        .rounded(px(theme::R_CHIP))
        .bg(rgb(theme::RAISED_2))
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_2))
        .child(key.into())
}

/// Key hints as `key verb   key verb`: keys `TEXT_2`, verbs `TEXT_MUTED`,
/// `SPACE_3` between pairs and no separator glyph.
pub fn key_hints(hints: &[(&str, &str)]) -> Div {
    text_meta()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_3))
        .children(hints.iter().map(|(key, verb)| {
            // A pair never shrinks: a narrow row drops whole hints rather
            // than cutting one mid-word.
            div()
                .flex()
                .flex_shrink_0()
                .gap(px(theme::SPACE_1))
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_2))
                        .child(SharedString::from(key.to_string())),
                )
                .child(SharedString::from(verb.to_string()))
        }))
}

/// A chip's meaning. Colour is state: only the state tones carry a hue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Running,
    Attention,
    Blocked,
}

/// A tone's ink and ground (the ground as `0xRRGGBBAA`).
pub fn tone_inks(tone: Tone) -> (u32, u32) {
    match tone {
        Tone::Neutral => (theme::TEXT_2, (theme::RAISED_2 << 8) | 0xff),
        Tone::Accent => (theme::ACCENT, theme::ACCENT_WASH),
        Tone::Running => (theme::RUNNING, theme::RUNNING_WASH),
        Tone::Attention => (theme::ATTENTION, theme::ATTENTION_WASH),
        Tone::Blocked => (theme::BLOCKED, theme::BLOCKED_WASH),
    }
}

/// A chip: `CHIP_H`, `R_CHIP`, mono `FS_SM`, inked and grounded by its tone.
pub fn chip(label: impl Into<SharedString>, tone: Tone) -> Div {
    let (ink, ground) = tone_inks(tone);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::CHIP_H))
        .px(px(theme::CHIP_PAD_X))
        .rounded(px(theme::R_CHIP))
        .bg(rgba(ground))
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(ink))
        .child(label.into())
}

/// The prompt mark `❯`, drawn (Geist Mono lacks the glyph): `prompt.svg` in a
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

/// An empty surface's one line of guidance, centred: the title in `TEXT_2`
/// and an optional hint in `TEXT_MUTED` beneath it.
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

/// An icon-only control: `ICON_BUTTON` square, the glyph at
/// `ICON_BUTTON_GLYPH` in `TEXT_MUTED`, a tooltip naming what it does.
pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: &'static str,
    tooltip: &'static str,
    cx: &App,
) -> Button {
    button(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(theme::TEXT_MUTED).into())
                .hover(rgb(theme::HOVER).into())
                .active(rgb(theme::PRESSED).into()),
        )
        .size(px(theme::ICON_BUTTON))
        .tooltip(tooltip)
        .accessibility_label(tooltip)
        .child(icons::icon(
            glyph,
            theme::ICON_BUTTON_GLYPH,
            theme::TEXT_MUTED,
        ))
}

/// A quiet text control: `CONTROL_H`, mono `FS_UI` `W_LABEL` `TEXT_2`;
/// hover `RAISED_2`, press `FILL_HOVER`.
pub fn ghost_button(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    button(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(theme::TEXT_2).into())
                .hover(rgb(theme::RAISED_2).into())
                .active(rgb(theme::FILL_HOVER).into()),
        )
        .h(px(theme::CONTROL_H))
        .px(px(theme::CONTROL_PAD_X))
        .child(
            text_ui()
                .font_weight(theme::W_LABEL)
                .text_color(rgb(theme::TEXT_2))
                .child(label.into()),
        )
}

/// A destructive action: a ghost at rest (colour is state), `BLOCKED` ink.
pub fn danger_button(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    button(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(theme::BLOCKED).into())
                .hover(rgba(theme::BLOCKED_WASH).into())
                .active(rgba(theme::BLOCKED_WASH).into()),
        )
        .h(px(theme::CONTROL_H))
        .px(px(theme::CONTROL_PAD_X))
        .child(
            text_ui()
                .font_weight(theme::W_LABEL)
                .text_color(rgb(theme::BLOCKED))
                .child(label.into()),
        )
}

/// One option of a segmented choice. Its 1px edge is always in layout: rest
/// `HAIRLINE_STRONG` with `TEXT_2`; selected `ACCENT_WASH` + `ACCENT_EDGE` with
/// `TEXT_STRONG`.
pub fn choice_chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> Button {
    let (ink, ground, edge) = choice_inks(selected);
    button(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(ink).into())
                .hover(rgb(theme::HOVER).into())
                .active(rgb(theme::PRESSED).into()),
        )
        .h(px(theme::CHIP_H))
        .px(px(theme::CHIP_PAD_X))
        .rounded(px(theme::R_CHIP))
        .border_1()
        .border_color(rgba(edge))
        .when_some(ground, |chip, ground| chip.bg(rgba(ground)))
        .child(text_ui().text_color(rgb(ink)).child(label.into()))
}

/// A choice chip's ink, ground (`0xRRGGBBAA`) and edge.
pub fn choice_inks(selected: bool) -> (u32, Option<u32>, u32) {
    if selected {
        (
            theme::TEXT_STRONG,
            Some(theme::ACCENT_WASH),
            theme::ACCENT_EDGE,
        )
    } else {
        (theme::TEXT_2, None, theme::HAIRLINE_STRONG)
    }
}

// ------------------------------------------------------------------ menus

/// The face a menu row's detail is set in: prose for descriptions, mono for
/// paths and tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Prose,
    Mono,
}

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
    pub detail: Option<(SharedString, Face)>,
    /// The key that does the same thing, faint at the right edge.
    pub shortcut: Option<SharedString>,
    pub checked: bool,
    /// Arms before it runs, and wears the blocked ink.
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
    pub fn detail(mut self, text: impl Into<SharedString>, face: Face) -> Self {
        self.detail = Some((text.into(), face));
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

/// The row state table, as data: rest, cursor, disabled, destructive, armed.
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
            label: theme::TEXT_STRONG,
            detail: theme::TEXT_2,
            shortcut: theme::TEXT_MUTED,
            ground: Some(theme::BLOCKED_WASH),
        };
    }
    let label = match (item.destructive, cursor) {
        (true, _) => theme::BLOCKED,
        (false, true) => theme::TEXT_STRONG,
        (false, false) => theme::TEXT,
    };
    RowInks {
        label,
        detail: if cursor {
            theme::TEXT_2
        } else {
            theme::TEXT_MUTED
        },
        shortcut: theme::TEXT_MUTED,
        ground: cursor.then_some((theme::FILL << 8) | 0xff),
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

/// A mono name column for `chars` characters, clamped between
/// `MENU_NAME_MIN_W` and `MENU_NAME_MAX_W`.
pub fn mono_column_w(chars: usize) -> f32 {
    (chars as f32 * theme::MONO_CELL).clamp(theme::MENU_NAME_MIN_W, theme::MENU_NAME_MAX_W)
}

/// A menu row's content with no id and no pointer role, for kit hosts
/// (`PopupMenuItem::element`) that own the row's interaction.
pub fn menu_row_content(item: &MenuItem, cursor: bool, armed: bool) -> Div {
    let inks = row_inks(item, cursor, armed);
    let label: SharedString = if armed {
        format!("Confirm: {}", item.label).into()
    } else {
        item.label.clone()
    };
    let highlights = if armed {
        Vec::new()
    } else {
        match_highlights(&item.matched, item.disabled)
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
                .when(armed, |label| label.font_weight(theme::W_LABEL))
                .when_some(item.label_w, |label, width| {
                    label.w(px(width)).flex_shrink_0()
                })
                .child(gpui::StyledText::new(label).with_highlights(highlights)),
        )
        .when_some(item.detail.clone(), |row, (detail, face)| {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(match face {
                        Face::Prose => theme::FONT_PROSE,
                        Face::Mono => theme::FONT_MONO,
                    })
                    .text_size(px(theme::FS_SM))
                    .text_color(rgb(inks.detail))
                    .child(detail),
            )
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
    let row = menu_row_content(item, cursor, armed).id(id);
    if item.disabled || armed {
        row
    } else if cursor {
        row.hover_carried()
    } else {
        row.hover_raised().press_raised()
    }
}

/// A menu section title: mono `FS_SM` `W_LABEL` `TEXT_MUTED`, an optional
/// leading mark and an optional note after it.
pub fn menu_section(
    title: impl Into<SharedString>,
    leading: Option<(&'static str, u32)>,
    note: Option<SharedString>,
) -> Div {
    text_meta()
        .flex()
        .items_end()
        .gap(px(theme::SPACE_2))
        .h(px(theme::MENU_SECTION_H))
        .px(px(theme::MENU_ROW_PAD_X))
        .pb(px(theme::SPACE_1))
        .cursor_default()
        .when_some(leading, |row, (path, ink)| {
            row.child(icons::icon(path, theme::ROW_ICON, ink))
        })
        .child(div().font_weight(theme::W_LABEL).child(title.into()))
        .children(note)
}

/// The one separator inside a menu: a full-bleed hairline.
pub fn menu_separator() -> Div {
    hairline()
        .my(px(theme::MENU_SEP_Y))
        .mx(px(-theme::FLOAT_PAD))
        .w_auto()
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

/// A menu's footer: a separator, then its key hints.
pub fn menu_footer(hints: &[(&str, &str)]) -> Div {
    div().flex().flex_col().child(menu_separator()).child(
        key_hints(hints)
            .h(px(theme::MENU_SECTION_H))
            .px(px(theme::MENU_ROW_PAD_X)),
    )
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
            item = item.detail(detail.clone(), Face::Mono);
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
    pub choices: Vec<Choice>,
    pub open: bool,
    pub return_focus: gpui::FocusHandle,
    pub on_open: OpenChanged,
    pub on_pick: Picked,
}

#[derive(Default)]
struct ChoiceMenuState {
    menu: Option<gpui::Entity<gpui::component::menu::PopupMenu>>,
    steps: usize,
    initialized: bool,
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
                        menu = menu.item(
                            PopupMenuItem::element(move |_, _| {
                                kit_row(menu_section(title.clone(), mark, None))
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
                            kit_row(menu_row_content(&item, false, false))
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
        let menu = retained.read(cx).menu.clone();
        let on_open = self.on_open;
        let mut popover = Popover::new(SharedString::from(format!("choice:{}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .anchor(gpui::Anchor::BottomLeft)
            .trigger(self.trigger)
            .open(self.open)
            .on_open_change(move |open, window, cx| on_open(*open, window, cx));
        if let Some(menu) = menu {
            popover = popover
                .track_focus(&menu.focus_handle(cx))
                .content(move |_, _, _| {
                    use gpui::base::ElementExt as _;
                    let retained = retained.clone();
                    let menu = menu.clone();
                    // The kit surface keeps its own hairline ring; the one
                    // float shadow lifts it like every other floating surface.
                    div()
                        .rounded(px(theme::R_BLOCK))
                        .shadow(float_shadow())
                        .child(menu.clone())
                        .on_prepaint(move |_, window, cx| {
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
                                    window.dispatch_action(
                                        Box::new(gpui::base::actions::SelectDown),
                                        cx,
                                    );
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
    row.flex_1().mx(px(-theme::MENU_ROW_PAD_X))
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

    #[test]
    fn the_float_shadow_is_the_far_then_the_near_layer() {
        let layers = float_shadow();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].color, rgba(theme::SHADOW_FAR).into());
        assert_eq!(layers[0].offset.y, px(theme::SHADOW_FAR_Y));
        assert_eq!(layers[0].blur_radius, px(theme::SHADOW_FAR_BLUR));
        assert_eq!(layers[0].spread_radius, px(theme::SHADOW_FAR_SPREAD));
        assert_eq!(layers[1].color, rgba(theme::SHADOW_NEAR).into());
        assert!(layers.iter().all(|layer| !layer.inset));
    }

    #[test]
    fn a_floating_surface_wears_the_raised_ground_the_strong_edge_and_the_shadow() {
        let mut surface = floating_surface();
        let style = surface.style();
        assert_eq!(style.background, Some(Fill::from(rgb(theme::MENU))));
        assert_eq!(
            style.border_color,
            Some(rgba(theme::HAIRLINE_STRONG).into())
        );
        assert_eq!(style.box_shadow, Some(float_shadow()));
        assert_eq!(style.text.font_family, Some(theme::FONT_MONO.into()));
    }

    #[test]
    fn type_openers_pair_each_size_with_its_line_box() {
        for (mut run, face, size, line, ink) in [
            (
                text_ui(),
                theme::FONT_MONO,
                theme::FS_UI,
                theme::LH_UI,
                theme::TEXT,
            ),
            (
                text_meta(),
                theme::FONT_MONO,
                theme::FS_SM,
                theme::LH_META,
                theme::TEXT_MUTED,
            ),
            (
                text_prose(),
                theme::FONT_PROSE,
                theme::FS_PROSE,
                theme::LH_PROSE,
                theme::TEXT,
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
        let shadow = &focus.box_shadow.expect("an inset outline")[0];
        assert!(shadow.inset);
        assert_eq!(shadow.color, solid(theme::FOCUS_RING));
        assert_eq!(shadow.spread_radius, px(2.));
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
            Some(Fill::from(rgb(theme::RAISED_2)))
        );
        let mut gutter = gutter(div(), theme::LH_UI);
        assert_eq!(gutter.style().size.width, Some(px(theme::GUTTER_W).into()));
    }

    #[test]
    fn colour_is_state_on_chips() {
        assert_eq!(tone_inks(Tone::Neutral).0, theme::TEXT_2);
        assert_eq!(tone_inks(Tone::Accent), (theme::ACCENT, theme::ACCENT_WASH));
        assert_eq!(
            tone_inks(Tone::Running),
            (theme::RUNNING, theme::RUNNING_WASH)
        );
        assert_eq!(
            tone_inks(Tone::Attention),
            (theme::ATTENTION, theme::ATTENTION_WASH)
        );
        assert_eq!(
            tone_inks(Tone::Blocked),
            (theme::BLOCKED, theme::BLOCKED_WASH)
        );
        assert_eq!(
            choice_inks(true),
            (
                theme::TEXT_STRONG,
                Some(theme::ACCENT_WASH),
                theme::ACCENT_EDGE
            )
        );
        assert_eq!(
            choice_inks(false),
            (theme::TEXT_2, None, theme::HAIRLINE_STRONG)
        );
    }

    #[test]
    fn menu_rows_follow_the_state_table() {
        let rest = MenuItem::new("Rename");
        let fill = (theme::FILL << 8) | 0xff;
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
        let delete = MenuItem::new("Delete Thread").destructive();
        assert_eq!(row_inks(&delete, false, false).label, theme::BLOCKED);
        assert_eq!(row_inks(&delete, true, false).label, theme::BLOCKED);
        let armed = row_inks(&delete, false, true);
        assert_eq!(
            (armed.label, armed.ground),
            (theme::TEXT_STRONG, Some(theme::BLOCKED_WASH))
        );
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
        assert_eq!(mono_column_w(1), theme::MENU_NAME_MIN_W);
        assert_eq!(mono_column_w(400), theme::MENU_NAME_MAX_W);
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
        assert_eq!(item.detail, Some(("1M".into(), Face::Mono)));
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

    #[test]
    fn durations_read_in_compact_units() {
        assert_eq!(duration_label(Duration::from_millis(340)).as_ref(), "0.3s");
        assert_eq!(
            duration_label(Duration::from_millis(8_200)).as_ref(),
            "8.2s"
        );
        assert_eq!(duration_label(Duration::from_secs(42)).as_ref(), "42s");
        assert_eq!(duration_label(Duration::from_secs(134)).as_ref(), "2m14s");
    }
}
