//! The float grammar (theme WP-E, the prototype's `.float`) and the
//! context menu built on it: what a right-click on a Thread, a Group, a
//! Project or a Pane offers. Drawing only, like `nav.rs` — the cockpit
//! decides the rows and runs the verbs.
//!
//! Every menu, picker, popover and card draws in this one grammar: the
//! float (`float`: `FLOAT_GROUND`, a 1px `FLOAT_EDGE`, square, the float
//! shadow, no inset), a head row (`head`), section rows (`section`), rows
//! on the 20px grid with a 2-cell gutter that holds the accent `❯` on the
//! cursor row (`row`, `item_row`), and a footer of key hints under a
//! `FLOAT_RULE` (`footer`), every word of it `TEXT_MUTED`. A short ladder's
//! chosen step is an inverse token (`token`). Floats are opaque (theme
//! WP-E, FL-15), so their rows wear the float inks (`FLOAT_SEL` on the
//! cursor row, `FLOAT_HOVER` under the pointer) and never a glass overlay.
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
/// key that does the same thing (`cmd-F`), drawn so the command key can be
/// a glyph box.
pub type Item = MenuItem;

// ------------------------------------------------------------ the grammar

/// The float: `components::floating_surface` with no inset of its own —
/// its rows carry their cell of padding. The caller states its width and
/// position.
pub fn float() -> Div {
    components::floating_surface().p(px(0.))
}

/// A float's head row: its title in `TEXT_STRONG` at `W_STRONG`, and at the
/// right whatever the caller adds (its command, a verb), in `TEXT_MUTED`.
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
        .text_color(rgb(TEXT_MUTED))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(W_STRONG)
                .text_color(rgb(TEXT_STRONG))
                .child(title.into()),
        )
}

/// A section row: its title in `TEXT_MUTED`, half a row above it, led by a
/// provider's logomark in its brand colour, an optional note after a `·`.
pub fn section(
    title: impl Into<SharedString>,
    logo: Option<(&'static str, u32)>,
    note: Option<SharedString>,
) -> Div {
    components::text_ui()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(CH))
        .h(px(FLOAT_ROW_H))
        .mt(px(FLOAT_SECTION_GAP))
        .px(px(FLOAT_PAD_X))
        .whitespace_nowrap()
        .cursor_default()
        .text_color(rgb(TEXT_MUTED))
        .when_some(logo, |line, (path, ink)| {
            line.child(icons::icon(path, STATUS_LOGO, ink))
        })
        // The title and its note are one run, a space either side of the
        // `·`, as the prototype sets them: no gap to snap, no run to round.
        .child({
            let title: SharedString = title.into();
            let words = match note {
                Some(note) => SharedString::from(format!("{title} \u{b7} {note}")),
                None => title,
            };
            div().min_w_0().truncate().child(words)
        })
}

/// The 2-cell gutter every row opens with: the accent `❯` (`W_STRONG`, the
/// drawn prompt mark) on the cursor row, empty on the rest.
pub fn gutter(cursor: bool) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(FLOAT_GUTTER))
        .h(px(FLOAT_ROW_H))
        .when(cursor, |cell| {
            cell.debug_selector(|| "float-cursor".into())
                .child(components::prompt_mark(ACCENT))
        })
}

/// What an armed destructive row adds to its label.
const ARMED_SEAM: &str = " \u{b7} ";
const ARMED_ASK: &str = "press again";

/// A row's inks: its label, and the ground (`FLOAT_SEL` on the cursor and
/// an armed row). Its description and key are always `TEXT_MUTED`.
fn label_ink(item: &Item, cursor: bool, armed: bool) -> u32 {
    if item.disabled {
        TEXT_MUTED
    } else if armed {
        BLOCKED
    } else if cursor {
        TEXT_STRONG
    } else {
        TEXT
    }
}

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
/// row's interaction: the gutter, the label (accent on its fuzzy matches,
/// never a weight), the description two cells after it, the key hard
/// right, and a green `✓` on the standing choice.
pub fn row_face(item: &Item, cursor: bool, armed: bool) -> Div {
    let ink = label_ink(item, cursor, armed);
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
    components::text_ui()
        .flex()
        .items_center()
        .min_w_0()
        .h(px(FLOAT_ROW_H))
        .px(px(FLOAT_PAD_X))
        .whitespace_nowrap()
        .when(armed || (cursor && !item.disabled), |row| row.bg(FLOAT_SEL))
        .text_color(rgb(ink))
        .child(gutter(cursor && !item.disabled && !armed))
        .when_some(item.leading, |row, (path, mark)| {
            row.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .w(px(FLOAT_GUTTER))
                    .child(icons::icon(path, ROW_ICON, mark)),
            )
        })
        .child({
            // The label holds its run's width, as the browser measures it.
            let cells = components::run_width(&label);
            div()
                .min_w_0()
                .truncate()
                .map(|cell| match item.label_w {
                    Some(width) => cell.w(px(width)).flex_shrink_0(),
                    None => cell.w(px(cells)).flex_shrink(1.),
                })
                .child(gpui::StyledText::new(label).with_highlights(highlights))
        })
        .map(|row| match item.detail.clone() {
            Some(detail) => {
                let cell = div()
                    .flex_1()
                    .min_w_0()
                    .ml(px(FLOAT_DETAIL_GAP))
                    .truncate()
                    .text_color(rgb(TEXT_MUTED));
                row.child(if item.mono {
                    cell.text_ellipsis_start().child(detail)
                } else {
                    cell.child(detail)
                })
            }
            None => row.child(div().flex_1()),
        })
        .when_some(item.shortcut.clone(), |row, keys| {
            row.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .ml(px(FLOAT_DETAIL_GAP))
                    .child(key_label(&keys, TEXT_MUTED)),
            )
        })
        .when(item.checked, |row| {
            row.child(
                // The `✓` typed at the row's end, as the prototype's `.ok`
                // follows its description.
                div()
                    .flex()
                    .flex_shrink_0()
                    .debug_selector(|| "float-check".into())
                    .child(components::glyph("\u{2713}", RUNNING)),
            )
        })
}

/// A row with its pointer role: `FLOAT_HOVER` under the pointer and the
/// press face; the cursor row keeps `FLOAT_SEL` under the pointer (hover
/// never moves the `❯` or lightens its bar); an armed or disabled row takes
/// none. Callers add selectors and handlers only.
pub fn item_row(id: impl Into<ElementId>, item: &Item, cursor: bool, armed: bool) -> Stateful<Div> {
    let id = id.into();
    let key = crate::pointer::hover_key(&id);
    let row = row_face(item, cursor, armed).id(id);
    if item.disabled || armed {
        row
    } else if cursor {
        row.float_cursor().press_float()
    } else {
        row.hover_float(key).press_float()
    }
}

/// An inert line in a float (loading, empty, a refusal): one row in
/// `TEXT_MUTED` on the rows' text edge.
pub fn note(text: impl Into<SharedString>) -> Div {
    components::text_ui()
        .flex()
        .flex_shrink_0()
        .items_center()
        .min_h(px(FLOAT_ROW_H))
        .pl(px(FLOAT_PAD_X + FLOAT_GUTTER))
        .pr(px(FLOAT_PAD_X))
        .cursor_default()
        .text_color(rgb(TEXT_MUTED))
        .child(text.into())
}

/// A float's footer: a `FLOAT_RULE` rule, then one row of key hints —
/// `↑↓ select · ⏎ open · esc` — all `TEXT_MUTED`, its `·` too (FL-14).
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

/// A footer row of already-joined words, all `TEXT_MUTED`.
pub fn footer_line(text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    footer_shell().child(text)
}

/// The footer's row with nothing in it yet: the `FLOAT_RULE` rule, half a
/// row above it, one row of `TEXT_MUTED`. For a footer whose keys are set
/// as key combinations (`⌘`, `components::key_combo`).
pub fn footer_shell() -> Div {
    components::text_ui()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(FLOAT_FOOT_H))
        .mt(px(FLOAT_SECTION_GAP))
        .px(px(FLOAT_PAD_X))
        .border_t_1()
        .border_color(FLOAT_RULE)
        .whitespace_nowrap()
        .overflow_hidden()
        .text_color(rgb(TEXT_MUTED))
}

/// One step of a short ladder (an effort level, a two-way setting): the
/// chosen one inverse — `ACCENT_STRONG` with `ON_ACCENT` at `W_STRONG` — the
/// rest `TEXT_MUTED`, `FLOAT_HOVER` under the pointer.
pub fn token(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    chosen: bool,
) -> Stateful<Div> {
    let id = id.into();
    let key = crate::pointer::hover_key(&id);
    // Exactly its cells and a cell either side: padded runs would snap
    // each pad from 7.8 to 8 and round each word up, drifting the ladder.
    let label: SharedString = label.into();
    let token = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(FLOAT_ROW_H))
        .w(px(components::run_width(&label) + 2.0 * TOKEN_PAD_X))
        .whitespace_nowrap()
        .child(components::cells(label));
    if chosen {
        token
            .bg(rgb(ACCENT_STRONG))
            .text_color(rgb(ON_ACCENT))
            .font_weight(W_STRONG)
    } else {
        token
            .text_color(rgb(TEXT_MUTED))
            .cursor_pointer()
            .hover_float(key)
            .press_float()
    }
}

// --------------------------------------------------------- context menu

/// The context menu's shell: the float, at least `MENU_W`, as wide as its
/// longest verb beside its shortcut so no verb is cut, a quarter row of air
/// above and below its rows. The caller positions it (`anchored`, which
/// keeps it inside the window).
pub fn shell() -> Div {
    float().min_w(px(MENU_W)).py(px(SPACE_1))
}

/// The space between two groups of rows: half a row, no rule.
pub fn gap() -> Div {
    div().flex_shrink_0().h(px(HALF_ROW))
}

/// One row, its shortcut hard right. `armed` is a destructive row waiting
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
                    .ml(px(FLOAT_DETAIL_GAP))
                    .child(key_label(&keys, TEXT_MUTED)),
            )
        },
    )
}

/// A tooltip in the float grammar: one row of the grid's type in a cell of
/// padding, at most `TOOLTIP_MAX_W` wide (a long path wraps), on the float
/// with its edge and shadow. It is also how a label that truncates keeps
/// its full value reachable.
pub fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |window, cx| {
        gpui::component::tooltip::Tooltip::new(text.clone())
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .px(px(TOOLTIP_PAD_X))
            .py(px(TOOLTIP_PAD_Y))
            .max_w(px(TOOLTIP_MAX_W))
            .rounded(px(R_CONTROL))
            .shadow(components::float_shadow())
            .build(window, cx)
    }
}

/// A tooltip that names a verb and its key (`Toggle sidebar ⌘B`,
/// `Close esc`, `Send ↵`): the label, then the key in `TEXT_MUTED`,
/// modifiers drawn as glyphs (`components::key_combo`). No parentheses,
/// and no suffix when `key` is `None` — a key is read from the key table
/// (`components::bound_chord`), never typed, so a tooltip never names a
/// key that would not act.
pub fn tooltip_with_key(
    label: impl Into<SharedString>,
    key: Option<impl Into<SharedString>>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let label = label.into();
    let key: Option<SharedString> = key.map(Into::into);
    move |window, cx| {
        let (label, key) = (label.clone(), key.clone());
        gpui::component::tooltip::Tooltip::element(move |_, _| {
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
                        .map(|key| components::key_combo(key, TEXT_MUTED).flex_shrink_0()),
                )
        })
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(LH_UI))
        .px(px(TOOLTIP_PAD_X))
        .py(px(TOOLTIP_PAD_Y))
        .max_w(px(TOOLTIP_MAX_W))
        .rounded(px(R_CONTROL))
        .shadow(components::float_shadow())
        .build(window, cx)
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

/// A short ladder under a picker's rows (the model picker's effort row,
/// the prototype's `.effort`): its label, its steps as inverse tokens, the
/// step in force, and what a step does — applied at once, from a click or
/// ←/→.
#[derive(Clone)]
pub struct Ladder {
    pub label: SharedString,
    pub steps: Vec<SharedString>,
    pub chosen: Option<usize>,
    pub on_step: Picked,
}

/// A picker in the float grammar (the prototype's `#picker`): a head
/// (`model  /model`), the choices as rows — sections led by a provider's
/// mark, the cursor's `❯` on its `FLOAT_SEL` bar, a green `✓` on the
/// standing choice — an optional ladder row, and a footer of the keys that
/// act. The kit `Popover` hangs it off its trigger; the menu owns its
/// keyboard (↑/↓ the cursor, ←/→ the ladder, ↵ picks, esc closes) in the
/// kit's own `PopupMenu` key context, so its bindings beat the bare enter
/// and escape. Rows keep the `choice-row-{index}` selectors.
#[derive(IntoElement)]
pub struct ChoiceMenu {
    pub id: SharedString,
    pub trigger: gpui::component::button::Button,
    /// Which corner of the menu meets the trigger: `BottomLeft` for a
    /// control at the left of its row, `BottomRight` at the right, so the
    /// menu opens over its own Pane rather than across the next one.
    pub anchor: gpui::Anchor,
    pub choices: Vec<components::Choice>,
    pub open: bool,
    pub return_focus: gpui::FocusHandle,
    pub on_open: OpenChanged,
    pub on_pick: Picked,
    /// Where the menu may rest: its foot `MODEL_PICKER_GAP` above the
    /// Composer's band, its right edge inside the Pane's. `None` hangs it off
    /// the trigger alone.
    pub place: Option<components::FloatPlace>,
    /// How far left of its trigger the menu's left edge stands: the model
    /// picker's `MODEL_PICKER_NUDGE`, so its section logo sits left of the
    /// status logo; 0 lines the menu up with its trigger.
    pub lead: f32,
    /// The head row: the picker's name and the command that opens it.
    pub head: Option<(SharedString, SharedString)>,
    pub ladder: Option<Ladder>,
    /// The footer's key hints; none draws no footer.
    pub hints: Vec<(&'static str, &'static str)>,
    /// A fixed width (the model picker's 66 cells); else it hugs its rows
    /// between `CHOICE_MENU_MIN_W` and `SLASH_MENU_W`.
    pub width: Option<f32>,
}

/// A picker's trigger that never wears the selected face while its menu is
/// open (FL-13): the status segment stays a quiet segment under its float,
/// no raised box.
#[derive(IntoElement)]
struct QuietTrigger(gpui::component::button::Button);

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
        self.0
    }
}

#[derive(Default)]
struct ChoiceState {
    focus: Option<gpui::FocusHandle>,
    /// The cursor row (an index into the choices); `None` while closed.
    cursor: Option<usize>,
    /// The ladder step shown in force, moved by ←/→ ahead of the next
    /// frame's `chosen`.
    ladder: Option<usize>,
    initialized: bool,
    /// How far the menu is lifted and pulled left of where the kit hangs
    /// it, and whether that has been measured yet (unmeasured, it is drawn
    /// at zero opacity for its one frame).
    lift: f32,
    shift: f32,
    placed: bool,
}

fn live(choice: &components::Choice) -> bool {
    !choice.section && !choice.note && !choice.disabled
}

/// The live row `delta` steps from `from`, wrapping.
fn step_cursor(choices: &[components::Choice], from: Option<usize>, delta: isize) -> Option<usize> {
    let lives: Vec<usize> = (0..choices.len())
        .filter(|at| live(&choices[*at]))
        .collect();
    if lives.is_empty() {
        return None;
    }
    let at = from
        .and_then(|from| lives.iter().position(|live| *live == from))
        .map_or(0, |at| at as isize + delta);
    Some(lives[at.rem_euclid(lives.len() as isize) as usize])
}

struct ChoiceContent {
    choices: Vec<components::Choice>,
    head: Option<(SharedString, SharedString)>,
    ladder: Option<Ladder>,
    hints: Vec<(&'static str, &'static str)>,
    width: Option<f32>,
    on_pick: Picked,
    on_open: OpenChanged,
    return_focus: gpui::FocusHandle,
    place: Option<components::FloatPlace>,
    lead: f32,
    state: gpui::Entity<ChoiceState>,
    focus: gpui::FocusHandle,
    /// The view that draws the menu, redrawn when the menu's own state
    /// moves (the cursor, the ladder, its place): it alone, not the window.
    owner: gpui::EntityId,
}

impl ChoiceContent {
    /// Pick the row at `at` (a live one), hand the keyboard back, close.
    fn pick(self: &Rc<Self>, at: usize, window: &mut gpui::Window, cx: &mut gpui::App) {
        if !self.choices.get(at).is_some_and(live) {
            return;
        }
        (self.on_pick)(at, window, cx);
        self.return_focus.focus(window, cx);
        (self.on_open)(false, window, cx);
    }

    fn close(self: &Rc<Self>, window: &mut gpui::Window, cx: &mut gpui::App) {
        self.return_focus.focus(window, cx);
        (self.on_open)(false, window, cx);
    }

    fn build(self: &Rc<Self>, cx: &gpui::App) -> AnyElement {
        use gpui::base::actions::{Cancel, Confirm, SelectDown, SelectLeft, SelectRight, SelectUp};
        let (cursor, shown_step) = {
            let state = self.state.read(cx);
            (
                state.cursor,
                state
                    .ladder
                    .or_else(|| self.ladder.as_ref().and_then(|ladder| ladder.chosen)),
            )
        };
        #[cfg(test)]
        testing::DRAWN.with(|drawn| {
            drawn.set(testing::Drawn {
                placed: self.state.read(cx).placed,
                cursor,
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
                move |_: &SelectUp, _, cx| {
                    menu.state.update(cx, |state, _| {
                        state.cursor = step_cursor(&menu.choices, state.cursor, -1);
                    });
                    cx.notify(menu.owner);
                }
            })
            .on_action({
                let menu = self.clone();
                move |_: &SelectDown, _, cx| {
                    menu.state.update(cx, |state, _| {
                        state.cursor = step_cursor(&menu.choices, state.cursor, 1);
                    });
                    cx.notify(menu.owner);
                }
            })
            .on_action({
                let menu = self.clone();
                move |_: &SelectLeft, window, cx| menu.step_ladder(-1, window, cx)
            })
            .on_action({
                let menu = self.clone();
                move |_: &SelectRight, window, cx| menu.step_ladder(1, window, cx)
            })
            .on_action({
                let menu = self.clone();
                move |_: &Confirm, window, cx| {
                    if let Some(at) = menu.state.read(cx).cursor {
                        menu.pick(at, window, cx);
                    }
                }
            })
            .on_action({
                let menu = self.clone();
                move |_: &Cancel, window, cx| menu.close(window, cx)
            })
            .on_mouse_down_out({
                let menu = self.clone();
                move |_, window, cx| (menu.on_open)(false, window, cx)
            });
        if let Some((title, command)) = &self.head {
            surface = surface.child(head(title.clone()).child(command.clone()));
        }
        let mut rows = div()
            .id("choice-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .max_h(px(MENU_MAX_H))
            .overflow_y_scroll();
        // A mark rides its section title; only a menu without sections marks
        // its rows.
        let marked = !self.choices.iter().any(|choice| choice.section);
        for (index, choice) in self.choices.iter().enumerate() {
            if choice.section {
                // A section names its group in the terminal's lowercase
                // (`claude`, `codex · …`).
                rows = rows.child(section(
                    SharedString::from(choice.label.to_lowercase()),
                    choice.icon,
                    choice.detail.clone(),
                ));
                continue;
            }
            if choice.note {
                rows = rows.child(note(choice.label.clone()));
                continue;
            }
            let mut item = MenuItem::new(choice.label.clone())
                .checked(choice.checked)
                .disabled(choice.disabled);
            if let Some(detail) = &choice.detail {
                item = item.detail(detail.clone());
            }
            if let (true, Some((path, ink))) = (marked, choice.icon) {
                item = item.leading(path, ink);
            }
            // The pointer paints its own row (`FLOAT_HOVER`) and leaves the
            // cursor where the arrows put it: only ↑↓ move the `❯`.
            let pick = self.clone();
            rows = rows.child(
                item_row(("choice-row", index), &item, cursor == Some(index), false)
                    .debug_selector(move || format!("choice-row-{index}"))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        pick.pick(index, window, cx);
                    }),
            );
        }
        surface = surface.child(rows);
        if let Some(ladder) = &self.ladder {
            let mut row = components::text_ui()
                .debug_selector(|| "choice-ladder".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .mt(px(FLOAT_SECTION_GAP))
                .pt(px(FLOAT_SECTION_GAP))
                .px(px(FLOAT_PAD_X))
                .border_t_1()
                .border_color(FLOAT_RULE)
                .whitespace_nowrap()
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(components::run_width(&ladder.label) + TOKEN_PAD_X))
                        .whitespace_nowrap()
                        .text_color(rgb(TEXT_MUTED))
                        .child(ladder.label.clone()),
                );
            for (at, step) in ladder.steps.iter().enumerate() {
                let menu = self.clone();
                row = row.child(
                    token(("choice-step", at), step.clone(), shown_step == Some(at))
                        .debug_selector(move || format!("choice-step-{at}"))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            menu.set_step(at, window, cx);
                        }),
                );
            }
            surface = surface.child(row);
        }
        if !self.hints.is_empty() {
            surface = surface.child(footer(&self.hints));
        }
        let menu = self.clone();
        components::on_bounds(surface, move |bounds, window, cx| {
            if let Some(place) = menu.place {
                // Inside the surface's 1px edge: add it back.
                let bounds = bounds.dilate(px(1.));
                let lead = menu.lead;
                let moved = menu.state.update(cx, |state, _| {
                    // Undo the offsets in force to find where the kit laid
                    // it, then solve from there: the foot
                    // `MODEL_PICKER_GAP` over the band, the left edge
                    // `lead` left of the trigger, the right edge inside
                    // the Pane.
                    let natural_bottom = f32::from(bounds.bottom()) + state.lift;
                    let natural_right = f32::from(bounds.right()) + state.shift;
                    let lift = natural_bottom - (place.floor - MODEL_PICKER_GAP);
                    let shift = (natural_right - place.limit_right).max(0.).max(lead);
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
                    // Measured in prepaint, where a notify is lost with the
                    // frame being drawn: the owner draws the menu in its
                    // place on the next.
                    let owner = menu.owner;
                    window.defer(cx, move |_, cx| cx.notify(owner));
                }
            }
            let first = menu.state.update(cx, |state, _| {
                let first = !state.initialized;
                state.initialized = true;
                first
            });
            if first {
                menu.focus.focus(window, cx);
            }
        })
        .into_any_element()
    }

    fn step_ladder(self: &Rc<Self>, delta: isize, window: &mut gpui::Window, cx: &mut gpui::App) {
        let Some(ladder) = &self.ladder else {
            return;
        };
        if ladder.steps.is_empty() {
            return;
        }
        let from = self
            .state
            .read(cx)
            .ladder
            .or(ladder.chosen)
            .map_or(0, |at| at as isize);
        let to = (from + delta).clamp(0, ladder.steps.len() as isize - 1) as usize;
        self.set_step(to, window, cx);
    }

    fn set_step(self: &Rc<Self>, at: usize, window: &mut gpui::Window, cx: &mut gpui::App) {
        let Some(ladder) = &self.ladder else {
            return;
        };
        self.state.update(cx, |state, _| state.ladder = Some(at));
        (ladder.on_step)(at, window, cx);
        cx.notify(self.owner);
    }
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
                state.ladder = None;
                state.initialized = false;
                state.placed = false;
                state.lift = 0.;
                state.shift = 0.;
            });
        } else if state.read(cx).cursor.is_none() {
            // The arrows start on the standing choice, so a bare ↵ keeps
            // everything as it is; else on the first live row.
            let start = self
                .choices
                .iter()
                .position(|choice| live(choice) && choice.checked)
                .or_else(|| self.choices.iter().position(live));
            state.update(cx, |state, _| state.cursor = start);
        }
        let (focus, lift, shift, placed) = {
            let state = state.read(cx);
            (
                state
                    .focus
                    .clone()
                    .expect("the menu's focus is made with it"),
                state.lift,
                state.shift,
                state.placed,
            )
        };
        let on_open = self.on_open.clone();
        let mut popover = Popover::new(SharedString::from(format!("choice:{}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .anchor(self.anchor)
            .trigger(QuietTrigger(self.trigger))
            .open(self.open)
            .on_open_change(move |open, window, cx| on_open(*open, window, cx));
        if self.place.is_some() {
            // The kit hangs the menu off the trigger; the measured offsets
            // move it onto the Composer's band and inside the Pane.
            popover = popover
                .bottom(px(lift))
                .left(px(-shift))
                .when(!placed, |popover| popover.opacity(0.));
        }
        if self.open {
            let content = Rc::new(ChoiceContent {
                choices: self.choices,
                head: self.head,
                ladder: self.ladder,
                hints: self.hints,
                width: self.width,
                on_pick: self.on_pick,
                on_open: self.on_open,
                return_focus: self.return_focus,
                place: self.place,
                lead: self.lead,
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
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct Drawn {
        /// Moved onto its measured place (else drawn at zero opacity).
        pub placed: bool,
        pub cursor: Option<usize>,
    }

    thread_local! {
        pub(super) static DRAWN: Cell<Drawn> = Cell::new(Drawn::default());
    }

    /// The open choice menu as last drawn.
    pub(crate) fn drawn() -> Drawn {
        DRAWN.with(Cell::get)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn a_live_row_is_a_button_and_a_disabled_one_is_not() {
        let live = Item::new("Rename").shortcut("↵");
        let mut drawn = row(0, &live, false);
        assert_eq!(drawn.style().mouse_cursor, Some(CursorStyle::PointingHand));
        let dead = Item::new("Reveal in Finder").disabled(true);
        let mut drawn = row(1, &dead, false);
        assert_eq!(drawn.style().mouse_cursor, None);
    }

    /// FL-14: a footer's words and seams are all `TEXT_MUTED` (no faint
    /// `·`), under the float's own rule.
    #[test]
    fn a_footer_is_one_muted_line_under_the_float_rule() {
        let mut drawn = footer(&[
            ("\u{2191}\u{2193}", "model"),
            ("\u{23ce}", "apply"),
            ("esc", ""),
        ]);
        assert_eq!(drawn.style().text.color, Some(rgb(TEXT_MUTED).into()));
        assert_eq!(drawn.style().border_color, Some(FLOAT_RULE.into()));
        // The cursor row keeps its bar under the pointer; another row only
        // takes the float's hover ink.
        let cursor = Item::new("Opus 5.5 (1M)");
        let mut drawn = item_row("c", &cursor, true, false);
        assert_eq!(drawn.style().background, Some(FLOAT_SEL.into()));
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
        assert_eq!(drawn.style().background, Some(FLOAT_SEL.into()));
        let mut calm = row(2, &delete, false);
        assert_eq!(calm.style().background, None);
        assert_eq!(
            components::row_inks(&delete, false, false).label,
            TEXT,
            "a destructive verb reads like any row before it arms"
        );
        assert_eq!(
            components::row_inks(&delete, false, true).label,
            BLOCKED,
            "armed, the word alone turns blocked"
        );
    }
}
