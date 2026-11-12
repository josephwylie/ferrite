//! The list's pieces past the cards (`docs/design/t3-reference/sidebar.md`
//! §1-6, §13): the list's frame and the spacer that anchors the shelves
//! over the foot, a shelf's header, the slim rows of the Snoozed and
//! Parked shelves and the Parked tail's `Show 25 more`, the empty list's
//! words, and the column's grain.

use std::sync::Arc;
use std::time::SystemTime;

use ferrite_core::ThreadId;
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, AnyElement, Div, Hsla, RenderImage, SharedString, Stateful};

use ferrite_ui::icons::{self, icon};
use ferrite_ui::theme::*;
use ferrite_ui::{components, motion};

use super::{card, chord_glyphs, fitted_in, Measure, NavTooltip, RowStatus, Shelf, ThreadRow};

/// The list (T3's `ul.flex.flex-col.gap-px.flex-1`): at least the scroll
/// area's height, so the spacer can push the shelves to its foot; rows 1px
/// apart; the ghosts of rows that left are placed over it.
pub fn body() -> Div {
    div()
        .debug_selector(|| "nav-list-body".into())
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_h_full()
        .gap(px(NAV_LIST_GAP))
}

/// What stands before the first shelf present (T3's `mt-auto` on it): the
/// list's slack, so the cards stack from the top and the shelves sit right
/// over the foot while the list is shorter than the column.
pub fn spacer() -> Div {
    div()
        .debug_selector(|| "nav-shelf-spacer".into())
        .flex_1()
}

/// A shelf's words.
pub fn shelf_label(shelf: Shelf) -> &'static str {
    match shelf {
        Shelf::Working => "Working",
        Shelf::Snoozed => "Snoozed",
        Shelf::Parked => "Parked",
    }
}

/// The selector and blend key of a shelf's header.
pub fn shelf_key(shelf: Shelf) -> SharedString {
    format!("nav-shelf-{}", shelf_label(shelf).to_lowercase()).into()
}

/// A shelf's header (T3's `CollapsibleSectionHeader`): 32px, 2px in from
/// the sides, 12/500 in the muted ink — the label, an invisible rule
/// taking the slack (a header with a count drops its leader), the count
/// flush right in tabular figures, and a 12px chevron, right when folded
/// and down when open, turning over 150ms. No ground under the pointer;
/// the keyboard's cursor is a 2px inset ring. The cockpit wires the press.
pub fn shelf_heading(shelf: Shelf, count: usize, open: bool, cursor: bool) -> Stateful<Div> {
    let key = shelf_key(shelf);
    let selector = key.clone();
    let name = shelf_label(shelf).to_lowercase();
    div()
        .id(key.clone())
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_SHELF_GAP))
        .mx(px(NAV_SHELF_INSET))
        .h(px(NAV_SHELF_H))
        .px(px(NAV_SHELF_PAD_X))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_NAV_META))
        .font_weight(W_LABEL)
        .text_color(rgb(INK_SUBTLE))
        .when(cursor, |heading| {
            heading.border_2().border_color(rgba(FOCUS_RING))
        })
        .child(
            div()
                .debug_selector({
                    let name = name.clone();
                    move || format!("nav-shelf-{name}-label")
                })
                .min_w_0()
                .truncate()
                .child(shelf_label(shelf)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(SPACE_2))
                .h(px(1.0))
                .invisible()
                .bg(rgba(NAV_BORDER)),
        )
        .child(
            components::tabular(div().child(count.to_string()))
                .debug_selector({
                    let name = name.clone();
                    move || format!("nav-shelf-{name}-count")
                })
                .font_weight(W_BODY),
        )
        .child(
            div()
                .debug_selector(move || format!("nav-shelf-{name}-chevron"))
                .flex_shrink_0()
                .child(motion::settled(
                    SharedString::from(format!("{key}-chevron")),
                    open,
                    motion::SHELF_CHEVRON,
                    |turn| components::chevron_at(turn, NAV_CHEVRON, INK_SUBTLE),
                )),
        )
}

/// What a slim row's right slot says at rest: a parked Thread's age, or a
/// snoozed one's wake time behind a blue alarm clock.
#[derive(Clone)]
pub enum SlimTime {
    Parked(SharedString),
    Snoozed(SharedString),
}

/// The blend a slim row's time-to-action crossfade and its badge ride,
/// apart from its fill's.
pub(crate) fn slim_fade_key(key: &str) -> SharedString {
    format!("{key}:slim").into()
}

/// A slim row (T3's `SidebarThreadRow` slim variant, the Snoozed and Parked
/// shelves): 36px, 10px in, its Project badge at 40% in greyscale, its
/// title, and at its right its time; under the pointer or the cursor the
/// badge comes up to colour and the time gives way to `action` (Revive,
/// Wake), over 150ms. Everything is the secondary label ink until then,
/// but the shown Thread's and a waiting one's title. The fills are a
/// card's. The cockpit wires the press.
pub fn slim_row(
    row: &ThreadRow,
    time: SlimTime,
    action: AnyElement,
    measure: &Measure,
) -> Stateful<Div> {
    slim(row, time, Some(action), measure, true)
}

/// A slim row that has left the list, for its ghost.
pub fn ghost_slim(row: &ThreadRow, time: SlimTime, measure: &Measure) -> Stateful<Div> {
    slim(row, time, None, measure, false)
}

fn slim(
    row: &ThreadRow,
    time: SlimTime,
    action: Option<AnyElement>,
    measure: &Measure,
    live: bool,
) -> Stateful<Div> {
    let thread = row.thread;
    let key = card::card_key(thread);
    let fade_key = slim_fade_key(&key);
    let hover = if live { motion::hover_t(&key) } else { 0.0 };
    let lit = if live { motion::hover_t(&fade_key) } else { 0.0 };
    #[cfg(test)]
    if live {
        super::testing::DRAWN_HOVER.with(|drawn| drawn.borrow_mut().insert(key.clone(), hover));
        super::testing::DRAWN_SLIM.with(|drawn| drawn.borrow_mut().insert(thread, lit));
    }
    let strong = row.selected || row.status == RowStatus::NeedsYou;
    let ink: Hsla = if strong {
        rgb(INK_STRONG).into()
    } else {
        motion::mix(rgb(INK_SECONDARY_LABEL).into(), rgb(INK_STRONG).into(), lit)
    };
    let fill: Option<Hsla> = if row.selected {
        Some(rgba(NAV_CARD_ACTIVE).into())
    } else if hover > 0.0 {
        Some(rgba(NAV_CARD_HOVER).into())
    } else {
        row.picked.then(|| rgba(NAV_CARD_SELECTED).into())
    };
    let ground = card::card_ground(&key, row.selected, row.picked);
    let title_w = INNER_SLIM_TITLE_W;
    let title = fitted_in(
        title_w,
        FS_NAV_TITLE,
        ink,
        W_BODY,
        row.name.clone(),
        super::cut_ground(measure, &row.name, FS_NAV_TITLE, W_BODY, title_w, ground),
    );
    let rest = match &time {
        SlimTime::Parked(age) => div()
            .flex()
            .items_center()
            .child(components::tabular(div().child(age.clone()))),
        SlimTime::Snoozed(wake) => div()
            .flex()
            .items_center()
            .gap(px(SPACE_1))
            .child(icon(icons::ALARM_CLOCK, NAV_ALARM_GLYPH, NAV_WORKING_INK))
            .child(components::tabular(div().child(wake.clone()))),
    };
    let slot = div()
        .relative()
        .flex()
        .flex_shrink_0()
        .ml_auto()
        .justify_end()
        .items_center()
        .h(px(NAV_SLIM_SLOT_H))
        .min_w(px(NAV_SLIM_SLOT_MIN_W))
        .text_size(px(FS_NAV_META))
        .text_color(rgb(INK_SECONDARY_LABEL))
        .child(rest.opacity(1.0 - lit))
        .children(action.filter(|_| lit > 0.0).map(|action| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .flex()
                .items_center()
                .opacity(lit)
                .child(action)
        }));
    let frame = if live {
        div()
            .id(("nav-slim", thread.get() as usize))
            .debug_selector(move || format!("nav-thread-{}", thread.get()))
            .on_hover({
                let snap = motion::hover_snap(key.clone());
                let fade =
                    motion::hover_listener_with(fade_key, motion::SLIM_ACTION, motion::SLIM_ACTION);
                move |hovered, window, cx| {
                    snap(hovered, window, cx);
                    fade(hovered, window, cx);
                }
            })
    } else {
        div().id(("nav-slim-ghost", thread.get() as usize))
    };
    frame
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_SLIM_GAP))
        .h(px(NAV_SLIM_H))
        .px(px(NAV_SLIM_PAD_X))
        .rounded(px(R_CONTROL))
        .overflow_hidden()
        .whitespace_nowrap()
        .when_some(fill, |row, fill| row.bg(fill))
        .children(row.project.as_ref().map(|project| {
            ferrite_ui::project_badge::project_badge_saturated(project, lit)
                .opacity(motion::lerp(NAV_SLIM_BADGE_OPACITY, 1.0, lit))
        }))
        .child(title)
        .child(slot)
}

/// What a slim row's title is left: the row, less its padding, its badge
/// and its slot, each with its gap.
const INNER_SLIM_TITLE_W: f32 = super::INNER_W
    - 2.0 * NAV_SLIM_PAD_X
    - NAV_BADGE
    - NAV_SLIM_SLOT_MIN_W
    - 2.0 * NAV_SLIM_GAP;

/// A parked row's Revive (T3's Un-settle): `Undo2` at 14px.
pub fn revive_button(thread: ThreadId) -> Stateful<Div> {
    slim_action("nav-revive", thread, icons::UNDO_2, NAV_REVIVE_GLYPH, "Revive")
}

/// A snoozed row's Wake: `AlarmClockOff` at 12px.
pub fn wake_button(thread: ThreadId) -> Stateful<Div> {
    slim_action("nav-wake", thread, icons::ALARM_CLOCK_OFF, NAV_WAKE_GLYPH, "Wake thread")
}

fn slim_action(
    name: &'static str,
    thread: ThreadId,
    glyph: &'static str,
    size: f32,
    tip: &'static str,
) -> Stateful<Div> {
    let key = SharedString::from(format!("{name}-{}", thread.get()));
    let selector = key.clone();
    let t = motion::hover_t(&key);
    div()
        .id(key.clone())
        .debug_selector(move || selector.to_string())
        .flex()
        .items_center()
        .justify_center()
        .size(px(NAV_SLIM_SLOT_H))
        .rounded(px(R_CONTROL))
        .when(t > 0.0, |door| door.bg(rgba(NAV_CARD_HOVER)))
        .on_hover(motion::hover_snap(key))
        .nav_tooltip(tip)
        .child(icon(glyph, size, INK_STRONG))
}

/// The Parked tail's paging row (T3's `Show 25 more`): 36px, 10px in, a
/// 16px chevron leading — revealing more is not adding — in the muted
/// ink, the hover fill and strong ink under the pointer.
pub fn show_more_parked() -> Stateful<Div> {
    const KEY: &str = "nav-parked-more";
    let blend = SharedString::from(KEY);
    let t = motion::hover_t(&blend);
    div()
        .id(KEY)
        .debug_selector(|| KEY.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_SLIM_GAP))
        .h(px(NAV_SLIM_H))
        .px(px(NAV_SLIM_PAD_X))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_NAV_TITLE))
        .text_color(motion::mix(rgb(INK_SUBTLE).into(), rgb(INK_STRONG).into(), t))
        .when(t > 0.0, |row| row.bg(rgba(NAV_CARD_HOVER)))
        .on_hover(motion::hover_snap(blend))
        .child(icon(icons::CHEVRON_DOWN, NAV_SEARCH_GLYPH, INK_SUBTLE))
        .child(format!("Show {NAV_PARKED_PAGE} more"))
}

/// What a slim Snoozed row says of its wake: how long until it, as a
/// card's age says how long since (`12m`, `3h`, `2d`).
pub fn until_wake(until: SystemTime, now: SystemTime) -> SharedString {
    let left = until.duration_since(now).unwrap_or_default();
    card::card_age(now, now + left)
}

/// What the empty list says (T3's): `No threads yet`, `No threads in
/// {project} yet` when the filter names one.
pub fn empty_words(project: Option<&str>) -> String {
    match project {
        Some(project) => format!("No threads in {project} yet"),
        None => "No threads yet".to_string(),
    }
}

/// The empty list (T3's), centred: its words at 12px in the muted ink at
/// 60%, and under them Ferrite's way forward — the Parked shelf's link
/// when it holds Threads, else the new-thread chord.
pub fn empty(key: &'static str, words: impl Into<SharedString>, parked: Option<AnyElement>) -> Div {
    let hint: Option<AnyElement> = match parked {
        Some(parked) => Some(parked),
        None => components::bound_chord("cockpit::NewThread").map(|keys| {
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_1))
                .child(chord_glyphs(&keys))
                .child("New thread")
                .into_any_element()
        }),
    };
    div()
        .debug_selector(move || key.into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_EMPTY_GAP))
        .px(px(NAV_EMPTY_PAD_X))
        .py(px(NAV_EMPTY_PAD_Y))
        .text_size(px(FS_NAV_META))
        .text_color(rgba((INK_SUBTLE << 8) | (NAV_EMPTY_OPACITY * 255.0).round() as u32))
        .child(div().text_center().child(words.into()))
        .children(hint)
}

/// The empty list's way to its parked Threads: `Show parked threads`,
/// strong under the pointer. Its press unfolds the Parked shelf.
pub fn parked_link() -> Stateful<Div> {
    const KEY: &str = "nav-empty-parked";
    let blend = SharedString::from(KEY);
    let t = motion::hover_t(&blend);
    div()
        .id(KEY)
        .debug_selector(|| KEY.into())
        .when(t > 0.0, |link| link.text_color(rgb(INK_STRONG)))
        .on_hover(motion::hover_snap(blend))
        .child("Show parked threads")
}

thread_local! {
    static GRAIN: Arc<RenderImage> = grain_tile();
}

/// T3's surface grain: one tile of noise, white at random levels, its
/// alpha baked to `NAV_GRAIN_OPACITY` (gpui paints an image at full
/// opacity). BGRA, as `RenderImage` holds it.
fn grain_tile() -> Arc<RenderImage> {
    let side = NAV_GRAIN_TILE as u32;
    let alpha = (NAV_GRAIN_OPACITY * 255.0).round() as u8;
    let mut seed: u32 = 0x9e37_79b9;
    let mut bytes = Vec::with_capacity((side * side * 4) as usize);
    for _ in 0..side * side {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let level = (seed >> 24) as u8;
        bytes.extend_from_slice(&[level, level, level, alpha]);
    }
    let buffer = image::RgbaImage::from_raw(side, side, bytes).expect("a square tile");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

/// Rounds its own corners to `radius`: gpui's content mask is a plain
/// rectangle, so an image paints past a rounded clip's corners.
pub fn grain(radius: f32) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let tile = GRAIN.with(Arc::clone);
            let side = px(NAV_GRAIN_TILE);
            let r = px(radius);
            let mut y = bounds.top();
            while y < bounds.bottom() {
                let mut x = bounds.left();
                while x < bounds.right() {
                    let at = gpui::Bounds {
                        origin: gpui::point(x, y),
                        size: gpui::size(side, side),
                    };
                    let shown = at.intersect(&bounds);
                    let (left, right) = (shown.left() == bounds.left(), shown.right() == bounds.right());
                    let (top, bottom) = (shown.top() == bounds.top(), shown.bottom() == bounds.bottom());
                    let corner = |on: bool| if on { r } else { px(0.) };
                    let radii = gpui::Corners {
                        top_left: corner(top && left),
                        top_right: corner(top && right),
                        bottom_right: corner(bottom && right),
                        bottom_left: corner(bottom && left),
                    };
                    let _ = window.paint_image(shown, at, radii, tile.clone(), 0, false);
                    x += side;
                }
                y += side;
            }
        },
    )
    .absolute()
    .inset_0()
}
