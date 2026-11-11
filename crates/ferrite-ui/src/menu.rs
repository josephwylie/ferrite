//! The float grammar (theme WP-E, SPEC §5.8 "Other floats") and the
//! context menu built on it: what a right-click on a Thread, a Group, a
//! Project or a Pane offers. Drawing only, like `nav.rs` — the cockpit
//! decides the rows and runs the verbs.
//!
//! Every menu, picker, popover and card draws in this one grammar, the
//! palette's shell at a smaller scale: the float (`float`: `paint::FLOAT`,
//! radius `R_FLOAT`, a 1px `FLOAT_EDGE` ring, the float shadow), a head
//! row (`head`), section headings (`section`), rows `FLOAT_ROW_H` high
//! inset `FLOAT_PAD` from the float's edge (`row`, `item_row`), the cursor
//! row a `FLOAT_SEL` fill with its label `INK_STRONG`, and a footer of key
//! hints under a `FLOAT_EDGE` rule (`footer`), every word of it
//! `INK_SUBTLE`. A short ladder's chosen step is a lit token (`token`).
//! Floats are opaque (theme WP-E, FL-15), so their rows wear the float
//! inks (`FLOAT_SEL` on the cursor row, `FLOAT_HOVER` under the pointer),
//! steps over `FLOAT`.
//!
//! A destructive verb never runs on one press: its row arms on the first
//! (the selection ground, the label `BLOCKED` and `· press again` after it)
//! and runs on the second. Anything else pressed disarms it.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, Div, ElementId, HighlightStyle, SharedString, Stateful};

use crate::components::{self, MenuItem};
use crate::icons;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

/// One row of a menu: the shared menu row's content. Its `shortcut` is the
/// key that does the same thing (`cmd-F`), drawn so the command key is a
/// glyph.
pub type Item = MenuItem;

// ------------------------------------------------------------ the grammar

/// The float: `components::floating_surface` with no inset of its own —
/// its rows hold their own (`FLOAT_PAD` round a row's fill, `FLOAT_PAD_X`
/// to a line's text). The caller states its width and position.
pub fn float() -> Div {
    components::floating_surface().p(px(0.))
}

/// A float's head row: its title in `INK_STRONG` at `W_LABEL`, and at the
/// right whatever the caller adds (its command, a verb), in `INK_SUBTLE`.
pub fn head(title: impl Into<SharedString>) -> Div {
    components::text_ui()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(FLOAT_DETAIL_GAP))
        .h(px(FLOAT_ROW_H))
        .px(px(FLOAT_PAD_X))
        .whitespace_nowrap()
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(W_LABEL)
                .text_color(rgb(INK_STRONG))
                .child(title.into()),
        )
}

/// A section heading, as the palette sets one at the float's scale: its
/// word in 12/500 `INK_SUBTLE` on the heading's foot, led by a provider's
/// logomark in its brand colour, an optional note after a `·`.
pub fn section(
    title: impl Into<SharedString>,
    logo: Option<(&'static str, u32)>,
    note: Option<SharedString>,
) -> Div {
    components::text_ui()
        .flex()
        .flex_shrink_0()
        .items_end()
        .gap(px(SPACE_1_5))
        .h(px(FLOAT_SECTION_H))
        .mt(px(FLOAT_SECTION_GAP))
        .pb(px(SPACE_1))
        .px(px(FLOAT_PAD_X))
        .whitespace_nowrap()
        .cursor_default()
        .text_size(px(FS_META))
        // The meta pair's line, so the word and its logo share one centre.
        .line_height(px(LH_META))
        .font_weight(W_LABEL)
        .text_color(rgb(INK_SUBTLE))
        .when_some(logo, |line, (path, ink)| {
            line.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(LH_META))
                    .child(icons::icon(path, STATUS_LOGO, ink)),
            )
        })
        .child({
            let title: SharedString = title.into();
            let words = match note {
                Some(note) => SharedString::from(format!("{title} \u{b7} {note}")),
                None => title,
            };
            div().min_w_0().truncate().child(words)
        })
}

/// What an armed destructive row adds to its label.
const ARMED_SEAM: &str = " \u{b7} ";
const ARMED_ASK: &str = "press again";

/// A key as a row draws it: a key table's spelling (`cmd-F`) through
/// `components::key_combo`, so the command key is a glyph box; anything
/// else (`↵`, `esc`) as its own word.
pub fn key_label(keys: &str, ink: u32) -> AnyElement {
    let spelled = ["cmd-", "alt-", "ctrl-", "shift-"]
        .iter()
        .any(|modifier| keys.contains(modifier));
    if spelled {
        components::key_combo(keys, ink).into_any_element()
    } else {
        div()
            .text_color(rgb(ink))
            .child(SharedString::from(keys.to_string()))
            .into_any_element()
    }
}

/// A row's face with no id and no pointer role, for kit hosts that own the
/// row's interaction: `FLOAT_ROW_H`, inset `MENU_ROW_INSET_X` from the
/// float's sides, radius `R_CONTROL`; a leading mark, the label (strong on
/// its fuzzy matches), the description after it, a `✓` on the standing
/// choice in `MENU_CHECK_INK`, and the key hard right (Claude's `✓ 5`). The cursor (or an
/// armed) row is `FLOAT_SEL`; its inks are `components::row_inks`.
pub fn row_face(item: &Item, cursor: bool, armed: bool) -> Div {
    let inks = components::row_inks(item, cursor, armed);
    let (ink, dim) = (inks.label, inks.detail);
    let (label, highlights): (SharedString, _) = if armed {
        let at = item.label.len();
        let text = format!("{}{ARMED_SEAM}{ARMED_ASK}", item.label);
        let seam = HighlightStyle {
            color: Some(rgb(TEXT_FAINT).into()),
            ..Default::default()
        };
        (text.into(), vec![(at..at + ARMED_SEAM.len(), seam)])
    } else {
        (
            item.label.clone(),
            components::match_highlights(&item.matched, item.disabled),
        )
    };
    let label = div()
        .min_w_0()
        .truncate()
        // An aligned column (the `/` picker's names) is the row's own UI
        // face, as Claude's: the cockpit measures it in that face.
        .when_some(item.label_w, |cell, width| {
            cell.w(px(width)).flex_shrink_0()
        })
        .child(gpui::StyledText::new(label).with_highlights(highlights));
    // A badge rides the label's line; a note takes a line of its own under
    // it, and the row grows to hold both.
    let label = match (&item.badge, &item.note) {
        (None, None) => label.into_any_element(),
        (badge, note) => div()
            .flex()
            .flex_col()
            .min_w_0()
            .flex_shrink(1.)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .min_w_0()
                    .child(label)
                    .children(badge.clone().map(badge_face)),
            )
            .children(note.clone().map(|note| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(FS_META))
                    .line_height(px(LH_META))
                    .text_color(rgb(dim))
                    .child(note)
            }))
            .into_any_element(),
    };
    components::text_ui()
        .flex()
        .items_center()
        .gap(px(MENU_ROW_GAP))
        .min_w_0()
        .h(px(if item.note.is_some() {
            FLOAT_ROW_NOTE_H
        } else {
            FLOAT_ROW_H
        }))
        .mx(px(MENU_ROW_INSET_X))
        .px(px(MENU_ROW_PAD_X))
        .rounded(px(R_CONTROL))
        .whitespace_nowrap()
        .when(armed || (cursor && !item.disabled), |row| {
            row.bg(paint::FLOAT_SEL)
        })
        .text_color(rgb(ink))
        .when_some(item.leading, |row, (path, mark)| {
            row.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .child(icons::icon(path, ROW_ICON, mark)),
            )
        })
        .child(label)
        .map(|row| match item.detail.clone() {
            Some(detail) => row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(dim))
                    .child(detail),
            ),
            None => row.child(div().flex_1()),
        })
        .when_some(item.tag.clone(), |row, tag| row.child(badge_face(tag)))
        .when(item.checked, |row| {
            row.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .debug_selector(|| "float-check".into())
                    .child(components::glyph("\u{2713}", MENU_CHECK_INK)),
            )
        })
        .when_some(item.shortcut.clone(), |row, keys| {
            row.child(div().flex().flex_shrink_0().child(key_label(&keys, dim)))
        })
}

/// A row's tag (`Recommended`, `Default`): the badge size in `INK_SUBTLE` in
/// a `TAG_RING` hairline at `R_CONTROL`, no fill (Claude's) — a word set
/// apart, not a button.
fn badge_face(text: SharedString) -> Div {
    div()
        .flex_shrink_0()
        .px(px(SPACE_1_5))
        .rounded(px(R_CONTROL))
        .border_1()
        .border_color(rgb(TAG_RING))
        .text_size(px(FS_BADGE))
        .line_height(px(LH_META))
        .text_color(rgb(INK_SUBTLE))
        .child(text)
}

/// A row with its pointer role: `FLOAT_HOVER` under the pointer and the
/// press face; the cursor row keeps `FLOAT_SEL` under the pointer (hover
/// never moves the cursor or lightens its fill); an armed or disabled row takes
/// none, a disabled one saying why in its tooltip. Callers add selectors
/// and handlers only.
pub fn item_row(id: impl Into<ElementId>, item: &Item, cursor: bool, armed: bool) -> Stateful<Div> {
    let id = id.into();
    let key = crate::pointer::hover_key(&id);
    let row = row_face(item, cursor, armed)
        .id(id)
        .when_some(item.why.clone(), |row, why| row.tooltip(tooltip(why)));
    if item.disabled || armed {
        row
    } else if cursor {
        row.float_cursor().press_float()
    } else {
        row.hover_float(key).press_float()
    }
}

/// An inert line in a float (loading, empty, a refusal): one row in
/// `INK_SUBTLE` on the rows' text edge.
pub fn note(text: impl Into<SharedString>) -> Div {
    components::text_ui()
        .flex()
        .flex_shrink_0()
        .items_center()
        .min_h(px(FLOAT_ROW_H))
        .px(px(FLOAT_PAD_X))
        .cursor_default()
        .text_color(rgb(INK_SUBTLE))
        .child(text.into())
}

/// A float's footer: a `FLOAT_EDGE` rule, then one row of key hints —
/// `↑↓ select · ⏎ open · esc` — all `INK_SUBTLE`, its `·` too (FL-14).
pub fn footer(hints: &[(&str, &str)]) -> Div {
    let text = hints
        .iter()
        .map(|(key, verb)| {
            if verb.is_empty() {
                key.to_string()
            } else {
                format!("{key} {verb}")
            }
        })
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
    footer_line(text)
}

/// A footer row of already-joined words, all `INK_SUBTLE`: cut with an
/// ellipsis where the row is narrower than its hints.
pub fn footer_line(text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    footer_shell().child(div().min_w_0().truncate().child(text))
}

/// The footer's row with nothing in it yet: `FLOAT_SECTION_GAP` of air,
/// the `FLOAT_EDGE` rule, one row of 12 `INK_SUBTLE`. For a footer whose
/// keys are set as key combinations (`⌘`, `components::key_combo`).
pub fn footer_shell() -> Div {
    components::text_ui()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(FLOAT_FOOT_H))
        .mt(px(FLOAT_SECTION_GAP))
        .px(px(FLOAT_PAD_X))
        .border_t_1()
        .border_color(paint::FLOAT_EDGE)
        .whitespace_nowrap()
        .overflow_hidden()
        .text_size(px(FS_META))
        .text_color(rgb(INK_SUBTLE))
}

/// One step of a short ladder (an effort level, a two-way setting), a
/// `TOKEN_H` pill `TOKEN_PAD_X` either side of its word: the chosen one
/// on the selection fill (`FLOAT_SEL`) in `INK_STRONG` — a segmented
/// control's lit step, the same lift a cursor row gets, not a second
/// primary button — the rest `INK_SUBTLE`, `FLOAT_HOVER` under the pointer.
/// The lit step changes fill and ink only, never weight: picking another
/// step never moves the steps beside it.
pub fn token(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    chosen: bool,
) -> Stateful<Div> {
    let id = id.into();
    let key = crate::pointer::hover_key(&id);
    let label: SharedString = label.into();
    let token = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(TOKEN_H))
        .px(px(TOKEN_PAD_X))
        .rounded(px(R_CONTROL))
        .whitespace_nowrap()
        .child(label);
    if chosen {
        token.bg(paint::FLOAT_SEL).text_color(rgb(INK_STRONG))
    } else {
        token
            .text_color(rgb(INK_SUBTLE))
            .cursor_pointer()
            .hover_float(key)
            .press_float()
    }
}

// --------------------------------------------------------- context menu

/// The context menu's shell: the float, at least `MENU_W`, as wide as its
/// longest verb beside its shortcut so no verb is cut, `FLOAT_PAD` of air
/// above and below its rows. The caller positions it (`anchored`, which
/// keeps it inside the window).
pub fn shell() -> Div {
    float().min_w(px(MENU_W)).py(px(FLOAT_PAD))
}

/// The rule between two groups of a context menu's rows, Claude's
/// (claude/row-menu.png): a `FLOAT_EDGE` hairline on the rows' text edge,
/// `MENU_RULE_GAP` of air above and below it.
pub fn rule() -> Div {
    div()
        .flex_shrink_0()
        .h(px(1.))
        .my(px(MENU_RULE_GAP))
        .mx(px(MENU_ROW_INSET_X + MENU_ROW_PAD_X))
        .bg(paint::FLOAT_EDGE)
}

/// One row, its key hard right (a one-letter mnemonic, Claude's `P`, in the
/// label's size and `INK_SUBTLE`). `armed` is a destructive row waiting
/// for its second press: `<label> · press again`, the label `BLOCKED`.
pub fn row(index: usize, item: &Item, armed: bool) -> Stateful<Div> {
    let keys = item.shortcut.clone();
    let face = MenuItem {
        shortcut: None,
        ..item.clone()
    };
    item_row(("context-menu-row", index), &face, false, armed).when_some(
        keys.filter(|_| !armed),
        |row, keys| {
            row.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .child(key_label(&keys, INK_SUBTLE)),
            )
        },
    )
}

/// A tooltip in the float grammar: one meta line, `TOOLTIP_PAD_X` and
/// `TOOLTIP_PAD_Y` in, at most `TOOLTIP_MAX_W` wide (a long path wraps), on
/// the float's ground and ring at `R_CONTROL` with the float shadow.
/// It is also how a label that truncates keeps its full value reachable.
/// It comes in over `motion::TOOLTIP_IN` (`entering`).
pub fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |window, cx| {
        let tip = gpui::component::tooltip::Tooltip::new(text.clone())
            .font_family(FONT_UI)
            .text_size(px(FS_META))
            .line_height(px(LH_META))
            .px(px(TOOLTIP_PAD_X))
            .py(px(TOOLTIP_PAD_Y))
            .max_w(px(TOOLTIP_MAX_W))
            .rounded(px(R_CONTROL))
            .border_color(paint::FLOAT_EDGE)
            .shadow(components::float_shadow())
            .build(window, cx);
        entering(tip, window, cx)
    }
}

/// A tooltip that names a verb and its key (`Toggle sidebar ⌘B`,
/// `Close esc`, `Send ↵`): the label, then the key in `INK_SUBTLE`,
/// modifiers drawn as glyphs (`components::key_combo`). No parentheses,
/// and no suffix when `key` is `None` — a key is read from the key table
/// (`components::bound_chord`), never typed, so a tooltip never names a
/// key that would not act.
pub fn tooltip_with_key(
    label: impl Into<SharedString>,
    key: Option<impl Into<SharedString>>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    keyed_tooltip(label, key, TipFace::BOARD)
}

/// What a tooltip's face and entrance are: the board's (13px over
/// `motion::TOOLTIP_IN`, a 200ms hand-off) or the nav's (T3's: 12px over
/// `motion::NAV_TOOLTIP_IN`, a 400ms hand-off).
#[derive(Clone, Copy)]
struct TipFace {
    size: f32,
    line: f32,
    entrance: crate::motion::MotionSpec,
    handoff: std::time::Duration,
}

impl TipFace {
    const BOARD: TipFace = TipFace {
        size: FS_META,
        line: LH_META,
        entrance: crate::motion::TOOLTIP_IN,
        handoff: gpui::TOOLTIP_HANDOFF,
    };
    const NAV: TipFace = TipFace {
        size: FS_NAV_TOOLTIP,
        line: LH_META,
        entrance: crate::motion::NAV_TOOLTIP_IN,
        handoff: std::time::Duration::from_millis(NAV_TOOLTIP_HANDOFF_MS),
    };
}

/// The nav's tooltip (T3's `ui/tooltip.tsx`): `tooltip`'s float at 12px,
/// fading and growing in over 150ms, the next one within 400ms there at
/// once. The element opens it after 150ms (`nav::NavTooltip`).
pub fn nav_tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    keyed_tooltip(text, None::<SharedString>, TipFace::NAV)
}

/// `nav_tooltip` naming a bound action's key, as `action_tooltip` does.
pub fn nav_action_tooltip(
    label: impl Into<SharedString>,
    action: &str,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    keyed_tooltip(label, components::bound_chord(action), TipFace::NAV)
}

fn keyed_tooltip(
    label: impl Into<SharedString>,
    key: Option<impl Into<SharedString>>,
    face: TipFace,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let label = label.into();
    let key: Option<SharedString> = key.map(Into::into);
    move |window, cx| {
        let (label, key) = (label.clone(), key.clone());
        let tip = gpui::component::tooltip::Tooltip::element(move |_, _| {
            // The label comes first: it wraps whole rather than cut, and
            // the key beside it never gives way either (a rail item's
            // tooltip is the one place its Thread's title is read).
            gpui::div()
                .flex()
                .items_center()
                .gap(px(CH))
                .child(gpui::div().min_w_0().child(label.clone()))
                .children(
                    key.as_ref()
                        .map(|key| components::key_combo(key, INK_SUBTLE).flex_shrink_0()),
                )
        })
        .font_family(FONT_UI)
        .text_size(px(face.size))
        .line_height(px(face.line))
        .px(px(TOOLTIP_PAD_X))
        .py(px(TOOLTIP_PAD_Y))
        .max_w(px(TOOLTIP_MAX_W))
        .rounded(px(R_CONTROL))
        .border_color(paint::FLOAT_EDGE)
        .shadow(components::float_shadow())
        .build(window, cx);
        entering_as(tip, face, window, cx)
    }
}

/// A tooltip card of two parts: `title` in `INK_SUBTLE`, then `body` in
/// `INK_STRONG`, wrapped at `width` (a skill's name, then its description).
pub fn tooltip_card(
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    width: f32,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let (title, body) = (title.into(), body.into());
    move |window, cx| {
        let (title, body) = (title.clone(), body.clone());
        let tip = gpui::component::tooltip::Tooltip::element(move |_, _| {
            gpui::div()
                .flex()
                .flex_col()
                .w(px(width - 2. * TOOLTIP_PAD_X))
                .child(
                    gpui::div()
                        .debug_selector(|| "skill-tip-name".into())
                        .text_color(rgb(INK_SUBTLE))
                        .child(title.clone()),
                )
                .child(
                    gpui::div()
                        .debug_selector(|| "skill-tip-description".into())
                        .text_color(rgb(INK_STRONG))
                        .child(body.clone()),
                )
        })
        .font_family(FONT_UI)
        .text_size(px(FS_META))
        .line_height(px(LH_META))
        .px(px(TOOLTIP_PAD_X))
        .py(px(TOOLTIP_PAD_Y))
        .max_w(px(width))
        .rounded(px(R_CONTROL))
        .border_color(paint::FLOAT_EDGE)
        .shadow(components::float_shadow())
        .build(window, cx);
        entering(tip, window, cx)
    }
}

/// A tooltip `tip` as it comes in: fading and growing from its top left
/// over `motion::TOOLTIP_IN` (Claude's 120ms), unless one was up within
/// `gpui::TOOLTIP_HANDOFF` — then, as Claude's do once one has shown, it is
/// there at once. A tooltip has no exit: gpui drops it on the frame the
/// pointer leaves.
fn entering(tip: gpui::AnyView, window: &gpui::Window, cx: &mut gpui::App) -> gpui::AnyView {
    entering_as(tip, TipFace::BOARD, window, cx)
}

fn entering_as(
    tip: gpui::AnyView,
    face: TipFace,
    window: &gpui::Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    let handoff = window.tooltip_shown_within(face.handoff, cx);
    cx.new(|_| Entering {
        tip,
        handoff,
        entrance: face.entrance,
    })
    .into()
}

struct Entering {
    tip: gpui::AnyView,
    handoff: bool,
    entrance: crate::motion::MotionSpec,
}

impl gpui::Render for Entering {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let tip = gpui::div().child(self.tip.clone());
        if self.handoff {
            tip.into_any_element()
        } else {
            crate::motion::tooltip_in_over("tooltip-in", self.entrance, tip).into_any_element()
        }
    }
}

/// `tooltip_with_key` for a bound action: its chord read from the key
/// table (`components::bound_chord`), dropped when nothing binds it.
pub fn action_tooltip(
    label: impl Into<SharedString>,
    action: &str,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    tooltip_with_key(label, components::bound_chord(action))
}

// ---------------------------------------------------------- choice menu

/// What a summoned picker hears back: its open state changing, and a pick
/// by index into its choices.
pub type OpenChanged = Rc<dyn Fn(bool, &mut gpui::Window, &mut gpui::App)>;
pub type Picked = Rc<dyn Fn(usize, &mut gpui::Window, &mut gpui::App)>;

/// A row that opens a second list beside the menu (Claude's model menu:
/// `Effort  Medium ›`, `More models ›`, each list in a float of its own):
/// its label, the choice in force at its right, the list it opens, that
/// list's width, and what a pick there does. Submenu rows sit under the
/// menu's rows, each past an inset hairline — or, marked, as rows of it.
/// A submenu's own `submenus` stand under its choices and open one list
/// further out (the `+` menu's `Plugins ›` › each plugin); theirs are
/// not drawn.
#[derive(Clone)]
pub struct Submenu {
    /// A row's own mark: the row then stands among the menu's rows, no
    /// divider over it (the `+` menu's `Connectors ›`).
    pub icon: Option<(&'static str, u32)>,
    pub label: SharedString,
    pub value: SharedString,
    pub choices: Vec<components::Choice>,
    pub submenus: Vec<Submenu>,
    pub width: f32,
    pub on_pick: Picked,
}

/// A picker in the float grammar: the choices as rows — sections led by a
/// provider's mark, the cursor row's `FLOAT_SEL` fill, a `✓` on the
/// standing choice, a digit on the rows that take one — and any `Submenu`
/// rows under them. No head and no key-hint footer: the keys are
/// on the rows (Claude's and Codex's menus). The kit `Popover` hangs it off
/// its trigger; the menu owns its keyboard (↑/↓ the cursor, → or ↵ opens
/// the submenu and ← closes it, ↵ picks, esc closes) in the kit's own
/// `PopupMenu` key context, so its bindings beat the bare enter and escape.
/// Rows keep the `choice-row-{index}` selectors, the submenu rows
/// `choice-submenu-row-{k}`, the open submenu's rows `choice-sub-{index}`. Every picker is one: the Composer toolbar's, and
/// the Subagents strip's `N done ▾`.
#[derive(IntoElement)]
pub struct ChoiceMenu {
    pub id: SharedString,
    pub trigger: gpui::component::button::Button,
    pub choices: Vec<components::Choice>,
    pub submenus: Vec<Submenu>,
    pub open: bool,
    pub return_focus: gpui::FocusHandle,
    pub on_open: OpenChanged,
    pub on_pick: Picked,
    /// Where the menu rests. Given one, the menu opens up from its trigger
    /// (a Composer toolbar chip, below the room it opens into) on its first
    /// frame: its foot `MODEL_PICKER_GAP` above the floor, its right edge at
    /// the trigger's (Claude's model and mode menus), kept inside the limit.
    /// Without one it drops below its trigger (the Subagents strip, across
    /// the card's top), where the kit hangs it.
    pub place: Option<components::FloatPlace>,
    /// A fixed width — a placed menu states one, so it lands whole on its
    /// first frame; else it hugs its rows between `CHOICE_MENU_MIN_W` and
    /// `SLASH_MENU_W`.
    pub width: Option<f32>,
    /// A placed menu hangs by its left edge at the trigger's left instead,
    /// its foot `PLUS_MENU_GAP` over the floor (Claude's `+` menu, which
    /// opens just over the box, never over the sidebar).
    pub hang_left: bool,
}

/// A picker's trigger that never wears the kit's selected face while its
/// menu is open (FL-13): it wears `CHIP_OPEN` instead, pressed, as Claude's
/// chip does under its menu. A placed menu's trigger reports its bounds, where the
/// menu hangs from.
#[derive(IntoElement)]
struct QuietTrigger {
    button: gpui::component::button::Button,
    measure: Option<gpui::Entity<ChoiceState>>,
}

impl gpui::component::Selectable for QuietTrigger {
    fn selected(self, _: bool) -> Self {
        self
    }

    fn is_selected(&self) -> bool {
        false
    }
}

impl RenderOnce for QuietTrigger {
    fn render(self, _: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        match self.measure {
            Some(state) => components::on_bounds(
                div().flex().min_w_0().child(self.button),
                move |bounds, _, cx| state.update(cx, |state, _| state.trigger = Some(bounds)),
            )
            .into_any_element(),
            None => self.button.into_any_element(),
        }
    }
}

#[derive(Default)]
struct ChoiceState {
    focus: Option<gpui::FocusHandle>,
    /// The cursor row (an index into the choices, or `choices.len() + k`
    /// for the `k`th submenu row); `None` while closed.
    cursor: Option<usize>,
    /// The open submenu's cursor (an index into its choices, then its own
    /// submenu rows); `None` while it is shut. The open submenu is the one
    /// whose row the cursor is on.
    sub: Option<usize>,
    /// The cursor in the list the open submenu's row under `sub` opens, one
    /// further out; `None` while that is shut.
    deep: Option<usize>,
    /// The keyboard has moved a cursor since the menu opened: only then is
    /// the cursor row filled. At rest no row is (Claude's menus): the ✓
    /// says what stands, and the pointer's hover fill says where it is.
    moved: bool,
    initialized: bool,
    /// The trigger as the last frame laid it out: a placed menu hangs from
    /// it, so it lands in place on its first frame.
    trigger: Option<gpui::Bounds<gpui::Pixels>>,
}

fn live(choice: &components::Choice) -> bool {
    !choice.section && !choice.disabled
}

/// The live row `delta` steps from `from` among `lives`, wrapping.
fn step_in(lives: &[usize], from: Option<usize>, delta: isize) -> Option<usize> {
    if lives.is_empty() {
        return None;
    }
    let at = from
        .and_then(|from| lives.iter().position(|live| *live == from))
        .map_or(0, |at| at as isize + delta);
    Some(lives[at.rem_euclid(lives.len() as isize) as usize])
}

/// Where the arrows start in a list: the standing choice, so a bare ↵
/// keeps everything as it is; else the first live row.
fn start(choices: &[components::Choice]) -> Option<usize> {
    choices
        .iter()
        .position(|choice| live(choice) && choice.checked)
        .or_else(|| choices.iter().position(live))
}

/// Where the arrows start in a list of `choices` then `submenus` rows:
/// as `start`, else on the first submenu row.
fn start_in(choices: &[components::Choice], submenus: &[Submenu]) -> Option<usize> {
    start(choices).or_else(|| (!submenus.is_empty()).then_some(choices.len()))
}

/// The submenu whose row `cursor` is on, with its index, in a list of
/// `choices` then `submenus` rows.
fn submenu_in<'a>(
    choices: &[components::Choice],
    submenus: &'a [Submenu],
    cursor: Option<usize>,
) -> Option<(usize, &'a Submenu)> {
    let k = cursor?.checked_sub(choices.len())?;
    submenus.get(k).map(|submenu| (k, submenu))
}

/// The rows ↑/↓ visit in such a list: the live choices, then the submenu
/// rows.
fn lives_in(choices: &[components::Choice], submenus: usize) -> Vec<usize> {
    (0..choices.len())
        .filter(|at| live(&choices[*at]))
        .chain((0..submenus).map(|k| choices.len() + k))
        .collect()
}

struct ChoiceContent {
    choices: Vec<components::Choice>,
    submenus: Vec<Submenu>,
    width: Option<f32>,
    on_pick: Picked,
    on_open: OpenChanged,
    return_focus: gpui::FocusHandle,
    place: Option<components::FloatPlace>,
    hang_left: bool,
    state: gpui::Entity<ChoiceState>,
    focus: gpui::FocusHandle,
    /// The view that draws the menu, redrawn when the menu's own state
    /// moves (the cursors): it alone, not the window.
    owner: gpui::EntityId,
}

impl ChoiceContent {
    /// The `k`th submenu row's place in the cursor's order: after the
    /// choices.
    fn sub_row(&self, k: usize) -> usize {
        self.choices.len() + k
    }

    /// The submenu whose row `cursor` is on, with its index.
    fn submenu_at(&self, cursor: Option<usize>) -> Option<(usize, &Submenu)> {
        submenu_in(&self.choices, &self.submenus, cursor)
    }

    /// The submenu open now: the one under the cursor, while one is open.
    fn open_submenu(&self, state: &ChoiceState) -> Option<&Submenu> {
        state.sub?;
        self.submenu_at(state.cursor).map(|(_, submenu)| submenu)
    }

    /// The list open one further out: the open submenu's own submenu under
    /// its cursor, while it is open.
    fn open_deep(&self, state: &ChoiceState) -> Option<&Submenu> {
        state.deep?;
        let submenu = self.open_submenu(state)?;
        submenu_in(&submenu.choices, &submenu.submenus, state.sub).map(|(_, deep)| deep)
    }

    /// The rows ↑/↓ visit: the live choices, then the submenu rows.
    fn lives(&self) -> Vec<usize> {
        lives_in(&self.choices, self.submenus.len())
    }

    fn update(self: &Rc<Self>, cx: &mut gpui::App, change: impl FnOnce(&Self, &mut ChoiceState)) {
        self.state.update(cx, |state, _| change(self, state));
        cx.notify(self.owner);
    }

    /// Pick the row at `at` (a live one), hand the keyboard back, close.
    fn pick(self: &Rc<Self>, at: usize, window: &mut gpui::Window, cx: &mut gpui::App) {
        if !self.choices.get(at).is_some_and(live) {
            return;
        }
        (self.on_pick)(at, window, cx);
        self.close(window, cx);
    }

    /// Pick the open submenu's row at `at`, hand the keyboard back, close.
    fn pick_sub(self: &Rc<Self>, at: usize, window: &mut gpui::Window, cx: &mut gpui::App) {
        let Some(submenu) = self.open_submenu(self.state.read(cx)) else {
            return;
        };
        if !submenu.choices.get(at).is_some_and(live) {
            return;
        }
        (submenu.on_pick)(at, window, cx);
        self.close(window, cx);
    }

    /// Pick the row at `at` in the list open furthest out, hand the
    /// keyboard back, close.
    fn pick_deep(self: &Rc<Self>, at: usize, window: &mut gpui::Window, cx: &mut gpui::App) {
        let Some(deep) = self.open_deep(self.state.read(cx)) else {
            return;
        };
        if !deep.choices.get(at).is_some_and(live) {
            return;
        }
        (deep.on_pick)(at, window, cx);
        self.close(window, cx);
    }

    fn close(self: &Rc<Self>, window: &mut gpui::Window, cx: &mut gpui::App) {
        self.return_focus.focus(window, cx);
        (self.on_open)(false, window, cx);
    }

    /// Open the `k`th submenu on its standing choice, the cursor on its
    /// row.
    fn open_sub(self: &Rc<Self>, k: usize, cx: &mut gpui::App) {
        let Some(submenu) = self.submenus.get(k) else {
            return;
        };
        let first = start_in(&submenu.choices, &submenu.submenus);
        self.update(cx, |menu, state| {
            state.cursor = Some(menu.sub_row(k));
            state.sub = first;
            state.deep = None;
        });
    }

    fn close_sub(self: &Rc<Self>, cx: &mut gpui::App) {
        if self.state.read(cx).sub.is_some() {
            self.update(cx, |_, state| {
                state.sub = None;
                state.deep = None;
            });
        }
    }

    /// Open the open submenu's own `j`th submenu, the cursor on its row.
    fn open_deep_at(self: &Rc<Self>, j: usize, cx: &mut gpui::App) {
        self.update(cx, |menu, state| {
            let Some(submenu) = menu.open_submenu(state) else {
                return;
            };
            let Some(deep) = submenu.submenus.get(j) else {
                return;
            };
            state.sub = Some(submenu.choices.len() + j);
            state.deep = start(&deep.choices);
        });
    }

    fn close_deep(self: &Rc<Self>, cx: &mut gpui::App) {
        if self.state.read(cx).deep.is_some() {
            self.update(cx, |_, state| state.deep = None);
        }
    }

    /// The open submenu's own submenu under its cursor, by index.
    fn deep_at_cursor(&self, state: &ChoiceState) -> Option<usize> {
        let submenu = self.open_submenu(state)?;
        submenu_in(&submenu.choices, &submenu.submenus, state.sub).map(|(j, _)| j)
    }

    fn step(self: &Rc<Self>, delta: isize, cx: &mut gpui::App) {
        self.update(cx, |menu, state| {
            if let (Some(deep), Some(at)) = (menu.open_deep(state), state.deep) {
                state.deep = step_in(&lives_in(&deep.choices, 0), Some(at), delta);
            } else if let (Some(submenu), Some(sub)) = (menu.open_submenu(state), state.sub) {
                let lives = lives_in(&submenu.choices, submenu.submenus.len());
                state.sub = step_in(&lives, Some(sub), delta);
            } else {
                state.cursor = step_in(&menu.lives(), state.cursor, delta);
            }
            state.moved = true;
        });
    }

    fn confirm(self: &Rc<Self>, window: &mut gpui::Window, cx: &mut gpui::App) {
        let (cursor, sub, deep, deep_row) = {
            let state = self.state.read(cx);
            (
                state.cursor,
                state.sub,
                state.deep,
                self.deep_at_cursor(state),
            )
        };
        match (deep, sub, deep_row, cursor, self.submenu_at(cursor)) {
            (Some(at), ..) => self.pick_deep(at, window, cx),
            (None, Some(_), Some(j), ..) => {
                self.update(cx, |_, state| state.moved = true);
                self.open_deep_at(j, cx);
            }
            (None, Some(sub), None, ..) => self.pick_sub(sub, window, cx),
            (None, None, _, _, Some((k, _))) => {
                self.update(cx, |_, state| state.moved = true);
                self.open_sub(k, cx);
            }
            (None, None, _, Some(at), None) => self.pick(at, window, cx),
            (None, None, _, None, None) => {}
        }
    }

    fn build(self: &Rc<Self>, cx: &gpui::App) -> AnyElement {
        use gpui::base::actions::{Cancel, Confirm, SelectDown, SelectLeft, SelectRight, SelectUp};
        let (cursor, sub, deep, trigger, moved) = {
            let state = self.state.read(cx);
            (
                state.cursor,
                state.sub,
                state.deep,
                state.trigger,
                state.moved,
            )
        };
        // The rows the keyboard's cursors fill: none at rest.
        let lit = cursor.filter(|_| moved);
        #[cfg(any(test, feature = "test-support"))]
        testing::DRAWN.with(|drawn| {
            drawn.set(testing::Drawn {
                cursor,
                sub,
                deep,
                lit,
            })
        });
        let mut surface = float()
            .debug_selector(|| "choice-menu".into())
            .key_context("PopupMenu")
            .track_focus(&self.focus)
            .map(|surface| match self.width {
                Some(width) => surface.w(px(width)),
                None => surface.min_w(px(CHOICE_MENU_MIN_W)).max_w(px(SLASH_MENU_W)),
            })
            .on_action({
                let menu = self.clone();
                move |_: &SelectUp, _, cx| menu.step(-1, cx)
            })
            .on_action({
                let menu = self.clone();
                move |_: &SelectDown, _, cx| menu.step(1, cx)
            })
            .on_action({
                let menu = self.clone();
                move |_: &SelectRight, _, cx| {
                    let state = menu.state.read(cx);
                    let deep_row = menu.deep_at_cursor(state);
                    match (
                        state.sub,
                        state.deep,
                        deep_row,
                        menu.submenu_at(state.cursor),
                    ) {
                        (None, _, _, Some((k, _))) => {
                            menu.update(cx, |_, state| state.moved = true);
                            menu.open_sub(k, cx);
                        }
                        (Some(_), None, Some(j), _) => {
                            menu.update(cx, |_, state| state.moved = true);
                            menu.open_deep_at(j, cx);
                        }
                        _ => {}
                    }
                }
            })
            .on_action({
                let menu = self.clone();
                move |_: &SelectLeft, _, cx| {
                    if menu.state.read(cx).deep.is_some() {
                        menu.close_deep(cx);
                    } else {
                        menu.close_sub(cx);
                    }
                }
            })
            .on_action({
                let menu = self.clone();
                move |_: &Confirm, window, cx| menu.confirm(window, cx)
            })
            .on_action({
                let menu = self.clone();
                move |_: &Cancel, window, cx| {
                    if menu.state.read(cx).deep.is_some() {
                        menu.close_deep(cx);
                    } else if menu.state.read(cx).sub.is_some() {
                        menu.close_sub(cx);
                    } else {
                        menu.close(window, cx);
                    }
                }
            })
            // A row that shows a key (`1`…`5` on the modes) is picked by it.
            .on_key_down({
                let menu = self.clone();
                move |event: &gpui::KeyDownEvent, window, cx| {
                    let stroke = &event.keystroke;
                    if stroke.modifiers.modified() {
                        return;
                    }
                    let at = menu.choices.iter().position(|choice| {
                        !choice.disabled && choice.shortcut.as_deref() == Some(stroke.key.as_str())
                    });
                    if let Some(at) = at {
                        cx.stop_propagation();
                        menu.pick(at, window, cx);
                    }
                }
            });
        let mut rows = div()
            .id("choice-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .py(px(FLOAT_PAD))
            .max_h(px(MENU_MAX_H))
            .overflow_y_scroll();
        // A mark rides its section title; only a menu without sections marks
        // its rows.
        let marked = !self.choices.iter().any(|choice| choice.section);
        for (index, choice) in self.choices.iter().enumerate() {
            if choice.section {
                rows = rows.child(section(
                    choice.label.clone(),
                    choice.icon,
                    choice.detail.clone(),
                ));
                continue;
            }
            let item = choice.item(marked);
            // The pointer paints its own row (`FLOAT_HOVER`) and leaves the
            // cursor where the arrows put it: only ↑↓ move it. Moving onto
            // another row shuts the submenu, as a native menu's does.
            let pick = self.clone();
            let shut = self.clone();
            rows = rows.child(
                item_row(("choice-row", index), &item, lit == Some(index), false)
                    .debug_selector(move || format!("choice-row-{index}"))
                    .when_some(choice.why.clone(), |row, why| row.tooltip(tooltip(why)))
                    .on_mouse_move(move |_, _, cx| shut.close_sub(cx))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        pick.pick(index, window, cx);
                    }),
            );
        }
        // A marked submenu row is one of the rows; a bare one stands under
        // them past its divider.
        let mut side = None;
        let mut further = None;
        let mut under = Vec::new();
        for (k, submenu) in self.submenus.iter().enumerate() {
            let open = sub.is_some() && cursor == Some(self.sub_row(k));
            let face = self.sub_row_face(k, submenu, lit, open);
            if submenu.icon.is_some() {
                rows = rows.child(face);
            } else {
                under.push(face);
            }
            if open {
                // The submenu's own submenu open under its cursor.
                let deep_open = deep
                    .is_some()
                    .then(|| submenu_in(&submenu.choices, &submenu.submenus, sub))
                    .flatten();
                side = Some(self.sub_surface(
                    submenu,
                    sub.filter(|_| moved),
                    deep_open.map(|(j, _)| j),
                ));
                further = deep_open
                    .map(|(_, deep_menu)| self.deep_surface(deep_menu, deep.filter(|_| moved)));
            }
        }
        surface = surface.child(rows).children(under);
        let menu = self.clone();
        let surface = components::on_bounds(surface, move |_, window, cx| {
            let first = menu.state.update(cx, |state, _| {
                let first = !state.initialized;
                state.initialized = true;
                first
            });
            if first {
                menu.focus.focus(window, cx);
            }
        });
        // The menu and its open submenu beside it, feet level; a press on
        // either is a press inside.
        let menus = div()
            .flex()
            .items_end()
            .gap(px(SUBMENU_GAP))
            .on_mouse_down_out({
                let menu = self.clone();
                move |_, window, cx| (menu.on_open)(false, window, cx)
            })
            .child(surface)
            .children(side)
            .children(further);
        let Some(place) = self.place else {
            return menus.into_any_element();
        };
        // Placed from the trigger the last frame laid out — a trigger is
        // drawn before it can open its menu — so the menu stands in place
        // on its first frame.
        let width = self.width.unwrap_or(CHOICE_MENU_MIN_W);
        let right = trigger
            .map_or(place.limit_right, |trigger| f32::from(trigger.right()))
            .min(place.limit_right);
        let left = match trigger {
            // Hung from the trigger's left edge, as far as the limit lets.
            Some(trigger) if self.hang_left => {
                f32::from(trigger.left()).min(place.limit_right - width)
            }
            _ => right - width,
        };
        // The `+` menu rests just over the box (Claude's 3-4pt); a chip's
        // menu stands off its chip.
        let floor = place.floor
            - if self.hang_left {
                PLUS_MENU_GAP
            } else {
                MODEL_PICKER_GAP
            };
        gpui::anchored()
            .anchor(gpui::Anchor::BottomLeft)
            .position(gpui::point(px(left), px(floor)))
            .snap_to_window_with_margin(px(GRID_PAD))
            .child(menus)
            .into_any_element()
    }

    /// The `k`th submenu's row: its label, the choice in force and a `›`
    /// at the right, under a hairline. The pointer resting on it opens the
    /// submenu; a click toggles it.
    fn sub_row_face(
        self: &Rc<Self>,
        k: usize,
        submenu: &Submenu,
        cursor: Option<usize>,
        open: bool,
    ) -> Div {
        let lit = cursor == Some(self.sub_row(k)) || open;
        let mut item = MenuItem::new(submenu.label.clone());
        if let Some((path, ink)) = submenu.icon {
            item = item.leading(path, ink);
        }
        let hover = self.clone();
        let toggle = self.clone();
        // A marked submenu row is one more of the menu's rows; a bare one
        // stands past a divider inset to the text's edges (Claude's).
        let row = div().flex_shrink_0();
        let row = if submenu.icon.is_some() {
            row
        } else {
            // The row above already ends `FLOAT_PAD` short of the rule,
            // so the rule takes air only under it: 4 + 1 + 4 between the
            // rows, Claude's 37 pitch (claude/model-menu.png).
            row.pb(px(FLOAT_PAD)).child(
                div()
                    .h(px(HAIRLINE_W))
                    .mx(px(FLOAT_PAD_X))
                    .mb(px(FLOAT_PAD))
                    .bg(paint::FLOAT_EDGE),
            )
        };
        row.child(
            item_row(("choice-submenu-row", k), &item, lit, false)
                .debug_selector(move || format!("choice-submenu-row-{k}"))
                .child(chevron_cell(&submenu.value, lit))
                .on_mouse_move(move |_, _, cx| {
                    if !open {
                        hover.open_sub(k, cx);
                    }
                })
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if open {
                        toggle.close_sub(cx);
                    } else {
                        toggle.open_sub(k, cx);
                    }
                }),
        )
    }

    /// The open submenu: its own float, `submenu.width` wide, a `✓` on the
    /// choice in force, `sub` the cursor row the keyboard fills; then its
    /// own submenu rows, each with its `›`, the one at `open` open beside
    /// it. Resting on one opens it; resting on a choice shuts it.
    fn sub_surface(
        self: &Rc<Self>,
        submenu: &Submenu,
        sub: Option<usize>,
        open: Option<usize>,
    ) -> Div {
        let mut rows = div()
            .id("choice-sub-rows")
            .flex()
            .flex_col()
            .py(px(FLOAT_PAD))
            .max_h(px(MENU_MAX_H))
            .overflow_y_scroll();
        for (index, choice) in submenu.choices.iter().enumerate() {
            let pick = self.clone();
            let shut = self.clone();
            rows = rows.child(
                item_row(
                    ("choice-sub", index),
                    &choice.item(false),
                    sub == Some(index),
                    false,
                )
                .debug_selector(move || format!("choice-sub-{index}"))
                .when_some(choice.why.clone(), |row, why| row.tooltip(tooltip(why)))
                .on_mouse_move(move |_, _, cx| shut.close_deep(cx))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    pick.pick_sub(index, window, cx);
                }),
            );
        }
        for (j, deep) in submenu.submenus.iter().enumerate() {
            let row = submenu.choices.len() + j;
            let lit = sub == Some(row) || open == Some(j);
            let is_open = open == Some(j);
            let hover = self.clone();
            let toggle = self.clone();
            rows = rows.child(
                item_row(
                    ("choice-sub-submenu", j),
                    &MenuItem::new(deep.label.clone()),
                    lit,
                    false,
                )
                .debug_selector(move || format!("choice-sub-submenu-{j}"))
                .child(chevron_cell(&deep.value, lit))
                .on_mouse_move(move |_, _, cx| {
                    if !is_open {
                        hover.open_deep_at(j, cx);
                    }
                })
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if is_open {
                        toggle.close_deep(cx);
                    } else {
                        toggle.open_deep_at(j, cx);
                    }
                }),
            );
        }
        float()
            .debug_selector(|| "choice-submenu".into())
            .w(px(submenu.width))
            .child(rows)
    }

    /// The list open furthest out (a plugin's commands): its own float,
    /// `deep.width` wide, `at` the cursor row the keyboard fills.
    fn deep_surface(self: &Rc<Self>, deep: &Submenu, at: Option<usize>) -> Div {
        let mut rows = div()
            .id("choice-deep-rows")
            .flex()
            .flex_col()
            .py(px(FLOAT_PAD))
            .max_h(px(MENU_MAX_H))
            .overflow_y_scroll();
        for (index, choice) in deep.choices.iter().enumerate() {
            let pick = self.clone();
            rows = rows.child(
                item_row(
                    ("choice-deep", index),
                    &choice.item(false),
                    at == Some(index),
                    false,
                )
                .debug_selector(move || format!("choice-deep-{index}"))
                .when_some(choice.why.clone(), |row, why| row.tooltip(tooltip(why)))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    pick.pick_deep(index, window, cx);
                }),
            );
        }
        float()
            .debug_selector(|| "choice-deep-menu".into())
            .w(px(deep.width))
            .child(rows)
    }
}

/// A submenu row's right end: the choice in force, then its `›`.
fn chevron_cell(value: &SharedString, lit: bool) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_1))
        .text_color(rgb(if lit { INK_STRONG } else { INK_SUBTLE }))
        .when(!value.is_empty(), |cell| cell.child(value.clone()))
        .child(icons::icon(icons::CHEVRON_RIGHT, ICON_CHEVRON, INK_SUBTLE))
}

impl RenderOnce for ChoiceMenu {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        use gpui::component::popover::Popover;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| ChoiceState {
            focus: Some(cx.focus_handle()),
            ..Default::default()
        });
        if !self.open {
            state.update(cx, |state, _| {
                state.cursor = None;
                state.sub = None;
                state.deep = None;
                state.moved = false;
                state.initialized = false;
            });
        } else if state.read(cx).cursor.is_none() {
            let first = start(&self.choices)
                .or_else(|| (!self.submenus.is_empty()).then_some(self.choices.len()));
            state.update(cx, |state, _| state.cursor = first);
        }
        let focus = state
            .read(cx)
            .focus
            .clone()
            .expect("the menu's focus is made with it");
        let on_open = self.on_open.clone();
        let mut popover = Popover::new(SharedString::from(format!("choice:{}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .trigger(QuietTrigger {
                // Pressed while its menu is open (Claude's chips).
                button: if self.open {
                    self.trigger.bg(paint::CHIP_OPEN)
                } else {
                    self.trigger
                },
                measure: self.place.map(|_| state.clone()),
            })
            .open(self.open)
            .on_open_change(move |open, window, cx| on_open(*open, window, cx));
        if self.open {
            let content = Rc::new(ChoiceContent {
                choices: self.choices,
                submenus: self.submenus,
                width: self.width,
                on_pick: self.on_pick,
                on_open: self.on_open,
                return_focus: self.return_focus,
                place: self.place,
                hang_left: self.hang_left,
                state,
                focus: focus.clone(),
                owner: window.current_view(),
            });
            popover = popover
                .track_focus(&focus)
                .content(move |_, _, cx| content.build(cx));
        }
        popover
    }
}

/// What the open choice menu was last drawn with: the stale-chrome tests
/// read what is on screen.
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Drawn {
        pub cursor: Option<usize>,
        /// The open submenu's cursor; `None` while it is shut.
        pub sub: Option<usize>,
        /// The cursor one list further out; `None` while that is shut.
        pub deep: Option<usize>,
        /// The row the cursor fills: none until the keyboard moves it.
        pub lit: Option<usize>,
    }

    thread_local! {
        pub(super) static DRAWN: Cell<Drawn> = Cell::new(Drawn::default());
    }

    /// The open choice menu as last drawn.
    pub fn drawn() -> Drawn {
        DRAWN.with(Cell::get)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disabled row says why in its tooltip and stays one line — a
    /// disabled Fork never wraps to "a turn is running; stop it first".
    #[test]
    fn a_disabled_rows_reason_is_its_tooltip_not_a_second_line() {
        let item = Item::new("Fork")
            .shortcut("F")
            .disabled_because("a turn is running; stop it first");
        assert!(item.disabled);
        assert_eq!(item.note, None);
        assert_eq!(
            item.why.as_deref(),
            Some("a turn is running; stop it first")
        );
        let mut row = row_face(&item, false, false);
        assert_eq!(row.style().size.height, Some(px(FLOAT_ROW_H).into()));
    }

    /// Every tooltip Ferrite draws is the measured one (`tooltip`, its
    /// keyed kin): nothing builds the kit's default `Tooltip` itself, and
    /// no kit button is handed bare text, which the kit draws in its own
    /// default (a kit button takes `tooltip_builder(tooltip(..))`).
    #[test]
    fn no_tooltip_falls_back_to_the_kits_default() {
        // Cargo runs a crate's tests in the crate's own directory.
        let crates = std::path::PathBuf::from("..");
        let mut stale = Vec::new();
        let mut dirs = vec![crates];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "rs") || path.ends_with("menu.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                for (at, line) in text.lines().enumerate() {
                    let code = line.trim_start();
                    if code.starts_with("//") {
                        continue;
                    }
                    if code.contains("Tooltip::new(")
                        || code.contains(".tooltip(\"")
                        || code.contains(".tooltip(format!")
                    {
                        stale.push(format!("{}:{}", path.display(), at + 1));
                    }
                }
            }
        }
        assert!(stale.is_empty(), "the kit's default tooltip at {stale:?}");
    }
    use gpui::{deferred, div, Context, CursorStyle, Render};
    use std::{cell::Cell, rc::Rc};

    struct OcclusionHarness {
        card_hovered: Rc<Cell<bool>>,
        menu_hovered: Rc<Cell<bool>>,
    }

    impl Render for OcclusionHarness {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let card_hovered = self.card_hovered.clone();
            let menu_hovered = self.menu_hovered.clone();
            div()
                .relative()
                .size(px(300.))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .on_mouse_move(move |_, _, _| card_hovered.set(true)),
                )
                .child(
                    deferred(
                        shell().absolute().top_0().left_0().child(
                            div()
                                .size(px(80.))
                                .on_mouse_move(move |_, _, _| menu_hovered.set(true)),
                        ),
                    )
                    .with_priority(2),
                )
        }
    }

    /// The `/` and `@` pickers' rows are UI text at the UI size, as
    /// Claude's are: their aligned name column and their details take the
    /// row's own face, never the code face (the owner's: skill names drawn
    /// mono, and cut, since the column is measured in the UI face).
    #[test]
    fn a_picker_row_is_ui_text_through_and_through() {
        let source = include_str!("menu.rs");
        let face = &source[source.find("pub fn row_face").unwrap()..];
        let face = &face[..face.find("\n}\n").unwrap()];
        assert!(!face.contains("FONT_CODE"), "a menu row names no code face");
        let item = Item::new("better-interface")
            .label_w(120.)
            .detail("User-invoked, cross-discipline interface work");
        let mut drawn = row_face(&item, false, false);
        assert_eq!(drawn.style().text.font_family.as_deref(), Some(FONT_UI));
        assert_eq!(drawn.style().text.font_size, Some(px(FS_UI).into()));
    }

    #[test]
    fn a_live_row_is_a_button_and_a_disabled_one_is_not() {
        let live = Item::new("Rename").shortcut("↵");
        let mut drawn = row(0, &live, false);
        assert_eq!(drawn.style().mouse_cursor, Some(CursorStyle::PointingHand));
        let dead = Item::new("Reveal in Finder").disabled(true);
        let mut drawn = row(1, &dead, false);
        assert_eq!(drawn.style().mouse_cursor, None);
    }

    /// FL-14: a footer's words and seams are all `INK_SUBTLE` (no faint
    /// `·`), under the float's own rule.
    #[test]
    fn a_footer_is_one_muted_line_under_the_float_rule() {
        let mut drawn = footer(&[
            ("\u{2191}\u{2193}", "model"),
            ("\u{23ce}", "apply"),
            ("esc", ""),
        ]);
        assert_eq!(drawn.style().text.color, Some(rgb(INK_SUBTLE).into()));
        assert_eq!(drawn.style().border_color, Some(paint::FLOAT_EDGE.into()));
        // The cursor row keeps its bar under the pointer; another row only
        // takes the float's hover ink.
        let cursor = Item::new("Opus 5.5 (1M)");
        let mut drawn = item_row("c", &cursor, true, false);
        assert_eq!(drawn.style().background, Some(paint::FLOAT_SEL.into()));
        let mut rest = item_row("r", &cursor, false, false);
        assert_eq!(rest.style().background, None);
    }

    #[test]
    fn the_floating_shell_masks_the_cursor_beneath_it() {
        let mut drawn = shell();
        assert_eq!(drawn.style().mouse_cursor, Some(CursorStyle::Arrow));
        assert_eq!(drawn.style().min_size.width, Some(px(MENU_W).into()));
    }

    #[gpui::test]
    fn the_context_menu_blocks_hover_on_the_card_beneath_it(cx: &mut gpui::TestAppContext) {
        let card_hovered = Rc::new(Cell::new(false));
        let menu_hovered = Rc::new(Cell::new(false));
        let (_, window) = cx.add_window_view({
            let card_hovered = card_hovered.clone();
            let menu_hovered = menu_hovered.clone();
            move |_, _| OcclusionHarness {
                card_hovered,
                menu_hovered,
            }
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.simulate_mouse_move(
            gpui::point(px(20.), px(20.)),
            None,
            gpui::Modifiers::default(),
        );

        assert!(menu_hovered.get(), "the pointer still reaches the menu");
        assert!(
            !card_hovered.get(),
            "the covered Thread card must not receive hover"
        );
    }

    #[test]
    fn an_armed_destructive_row_holds_the_fill_and_colours_its_word() {
        let delete = Item::new("Delete thread").destructive();
        let mut drawn = row(2, &delete, true);
        assert_eq!(drawn.style().background, Some(paint::FLOAT_SEL.into()));
        let mut calm = row(2, &delete, false);
        assert_eq!(calm.style().background, None);
        assert_eq!(
            components::row_inks(&delete, false, false).label,
            BLOCKED,
            "a destructive verb is red before it arms, as Claude's"
        );
        assert_eq!(
            components::row_inks(&delete, false, true).label,
            BLOCKED,
            "armed, the word alone turns blocked"
        );
    }
}
