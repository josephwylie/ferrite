//! The left nav bar (#21): every Thread on one column of glance rows —
//! running first in grid order, parked below — so the cockpit is navigable
//! without memorizing keys. It is a view, never the only door: everything a
//! row does (focus, revive) stays reachable from the keyboard.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads (`status()`, `pending()`, `todos()`) plus the
//! parked-row cache it rebuilds on park/revive — `Store::load` and
//! `Instruments::of` are banned here, which is what keeps the 24-Pane wall
//! smooth with the nav open. Click wiring stays in `cockpit.rs`, the same
//! split `pane_cell` uses. Rows keep stable positions and never re-sort: a
//! park or revive moves a row between sections — a real state change — and
//! nothing else moves anything.

use ferrite_core::groups::GroupId;
use ferrite_core::store::Provider;
use ferrite_core::transcript::{Status, Todos};
use ferrite_core::ThreadId;
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, Div, FontWeight, Pixels, SharedString, Stateful};

use crate::pointer::{Pointer, PointerPressed};
use crate::theme::{
    ACCENT, EDGE, FAIL, GOOD, GRID_PAD, HAIRLINE, IDLE, INK, INK_FAINT, INK_MUTED, INK_SECONDARY,
    INSET, POPOVER_PAD, TEXT_CHIP_SM, TEXT_ROW, TRANSPARENT, WAIT, WAIT_WASH,
};

/// The nav's two widths: the 208px column, and the 40px LED rail cmd-b
/// folds it to. `CockpitView::cell()` subtracts whichever is live, so the
/// nav is part of the semantic-zoom input — no special case.
pub const WIDTH: f32 = 208.0;
pub const RAIL_WIDTH: f32 = 40.0;

/// What the nav draws this frame. Running rows are rebuilt per frame from
/// O(1) reads (small, like the strip's labels); parked rows live in the
/// cockpit's cache because each one cost a `Store::peek`.
pub struct NavState {
    pub running: Vec<RunningRow>,
    /// Threads waiting on a Decision — the header's amber fragment.
    pub waiting: usize,
    pub collapsed: bool,
}

/// One running Thread's row: a Wall cell flattened to one line.
pub struct RunningRow {
    pub thread: ThreadId,
    pub name: SharedString,
    pub binding: SharedString,
    pub provider: &'static str,
    pub status: Status,
    /// A pending Decision: the amber `needs you` chip, and the rail halo.
    pub needs_you: bool,
    pub todos: Option<Todos>,
    pub focused: bool,
}

/// One parked Thread's row, cached: its log is not in memory, so everything
/// here came from one `Store::peek` header read at park/revive time.
pub struct ParkedRow {
    pub thread: ThreadId,
    pub name: SharedString,
    pub binding: SharedString,
    pub provider: &'static str,
}

/// The provider tag a 208px row has room for — the full `claude · fable`
/// chip belongs to the Pane header. Empty when the provider is unknowable
/// (an unreadable parked log): honesty over decoration.
pub fn provider_tag(provider: Option<Provider>) -> &'static str {
    match provider {
        Some(Provider::Claude) => "cl",
        Some(Provider::Codex) => "cx",
        None => "",
    }
}

/// The nav column itself: full height, one step below the Pane surface,
/// a 1px edge on the right. The rows are the caller's to append — clicks
/// are wired where the view state lives.
pub fn shell(collapsed: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(if collapsed { RAIL_WIDTH } else { WIDTH }))
        .bg(rgb(INSET))
        .border_r_1()
        .border_color(rgba(EDGE))
        // Rows past the window's height are clipped, not smeared over the
        // grid; a scrolling nav is not v1.
        .overflow_hidden()
}

/// The 34px header — aligned with the Cockpit strip. Expanded it says
/// `THREADS` and counts (`7 · 2 waiting`, the fragment amber when nonzero);
/// the rail keeps only the count. The spec's 0.10em tracking on these CAPS
/// labels is dropped: gpui 0.2.2 has no letter-spacing.
pub fn header(threads: usize, waiting: usize, collapsed: bool) -> Div {
    let row = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(34.))
        .text_size(px(10.));
    if collapsed {
        return row
            .justify_center()
            .text_color(rgb(INK_MUTED))
            .child(SharedString::from(threads.to_string()));
    }
    let mut count = div().flex().items_center().gap(px(4.)).child(
        div()
            .text_color(rgb(INK_MUTED))
            .child(SharedString::from(threads.to_string())),
    );
    if waiting > 0 {
        // The `·` is the seam between the two counts, not part of the amber
        // fragment (#22 A5).
        count = count
            .child(div().text_color(rgb(INK_FAINT)).child("·"))
            .child(
                div()
                    .text_color(rgb(WAIT))
                    .child(SharedString::from(format!("{waiting} waiting"))),
            );
    }
    row.justify_between()
        .px(px(10.))
        .child(div().text_color(rgb(INK_MUTED)).child("THREADS"))
        .child(count)
}

pub fn group_header_with_title(
    id: GroupId,
    title: impl IntoElement,
    count: usize,
    active: bool,
) -> Stateful<Div> {
    row_frame(("nav-group", id.get() as usize), active)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(TEXT_ROW))
        .text_color(rgb(INK_SECONDARY))
        .child("▸")
        .child(title)
        .child(div().flex_1())
        .child(
            div()
                .text_size(px(TEXT_CHIP_SM))
                .text_color(rgb(INK_FAINT))
                .child(count.to_string()),
        )
}

pub fn drag_badge(label: SharedString) -> Div {
    div()
        .bg(rgb(INSET))
        .border_1()
        .border_color(rgba(EDGE))
        .rounded_sm()
        .px(px(GRID_PAD))
        .py(px(POPOVER_PAD))
        .text_size(px(TEXT_ROW))
        .text_color(rgb(INK))
        .child(label)
}

/// One running Thread's 28px row: LED, name, binding, then provider and the
/// signal slot on the right. Focus is the 2px steel bar plus the EDGE
/// ground; urgency stays the chip's amber — position and urgency never
/// share a colour.
#[cfg(test)]
pub fn running_row(row: &RunningRow) -> Stateful<Div> {
    let ink = match row.status {
        Status::Idle => INK_SECONDARY,
        _ => INK,
    };
    running_row_with_title(row, thread_title_text(row.name.clone(), ink))
}

pub fn running_row_with_title(row: &RunningRow, title: impl IntoElement) -> Stateful<Div> {
    let led = led_color(row.status);
    let mut line = row_frame(("nav-run", row.thread.get() as usize), row.focused)
        .child(dot(px(6.), led))
        .child(title);
    // The chip carries the whole signal: a row wearing it drops the binding
    // and provider hints rather than squeezing all three to fragments
    // (#22 A3).
    if !row.needs_you {
        line = line
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(10.))
                    .text_color(rgb(INK_MUTED))
                    .child(row.binding.clone()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(9.5))
                    .text_color(rgb(INK_FAINT))
                    .child(row.provider),
            );
    } else {
        line = line.child(div().flex_1());
    }
    if row.needs_you {
        line = line.child(needs_you_chip());
    } else if let Some(todos) = row.todos {
        line = line.child(
            div()
                .flex_shrink_0()
                .text_size(px(10.))
                .text_color(rgb(INK_SECONDARY))
                .child(SharedString::from(format!(
                    "{}/{}",
                    todos.done, todos.total
                ))),
        );
    }
    line
}

/// A running Thread on the 40px rail: an 8px LED on a 24px pitch — nothing
/// to read, only to notice. A Decision dot keeps a 16px amber halo so
/// urgency still carries across the room.
pub fn running_dot(row: &RunningRow) -> Stateful<Div> {
    let cell = rail_cell(("nav-run", row.thread.get() as usize));
    let led = dot(px(8.), led_color(row.status));
    if row.needs_you {
        return cell.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(16.))
                .h(px(16.))
                .rounded_full()
                .bg(rgba(WAIT_WASH))
                .child(led),
        );
    }
    cell.child(led)
}

/// The divider between the sections: a hairline, and expanded a 22px CAPS
/// label counting the parked Threads.
pub fn parked_header(count: usize, collapsed: bool) -> Div {
    if collapsed {
        return div()
            .flex_shrink_0()
            .h(px(1.))
            .mx(px(8.))
            .my(px(4.))
            .bg(rgba(HAIRLINE));
    }
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(22.))
        .px(px(10.))
        .border_t_1()
        .border_color(rgba(HAIRLINE))
        .text_size(px(10.))
        .text_color(rgb(INK_MUTED))
        .child(SharedString::from(format!("PARKED — {count}")))
}

/// One parked Thread's row: hollow LED, muted ink, and no signal — its log
/// is not in memory, and the row must not pretend otherwise.
#[cfg(test)]
pub fn parked_row(row: &ParkedRow) -> Stateful<Div> {
    parked_row_with_title(row, thread_title_text(row.name.clone(), INK_MUTED))
}

pub fn parked_row_with_title(row: &ParkedRow, title: impl IntoElement) -> Stateful<Div> {
    row_frame(("nav-parked", row.thread.get() as usize), false)
        .child(hollow_dot(px(6.)))
        .child(title)
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(10.))
                .text_color(rgb(INK_FAINT))
                .child(row.binding.clone()),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(9.5))
                .text_color(rgb(INK_FAINT))
                .child(row.provider),
        )
}

#[cfg(test)]
pub fn thread_title_text(title: SharedString, color: u32) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(color))
        .child(title)
}

pub fn editable_group_title(id: GroupId, title: SharedString) -> Stateful<Div> {
    div()
        .id(("rename-group", id.get() as usize))
        .min_w_0()
        .truncate()
        .rounded_sm()
        .px(px(2.))
        .child(title)
        .hover_control()
}

pub fn editable_thread_title(thread: ThreadId, title: SharedString, color: u32) -> Stateful<Div> {
    div()
        .id(("rename-thread", thread.get() as usize))
        .min_w_0()
        .truncate()
        .rounded_sm()
        .px(px(2.))
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(color))
        .child(title)
        .hover_control()
}

/// A parked Thread on the rail: a smaller hollow dot below the divider.
pub fn parked_dot(row: &ParkedRow) -> Stateful<Div> {
    rail_cell(("nav-parked", row.thread.get() as usize)).child(hollow_dot(px(6.)))
}

/// The shared 28px row chrome: the 2px left slot the focus bar lives in
/// (transparent otherwise, so nothing shifts), the Row hover, and the
/// pressed shade. Focused rows skip the wash but keep the cursor — the
/// dimmer hover wash must not downgrade the EDGE ground focus already
/// painted (#26's one skip rule, law for every row).
fn row_frame(id: (&'static str, usize), focused: bool) -> Stateful<Div> {
    let line = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(28.))
        .gap(px(6.))
        // 8px padding beside the 2px bar slot keeps the spec's 10px inset.
        .pl(px(8.))
        .pr(px(10.))
        .border_l_2()
        .border_color(rgba(TRANSPARENT));
    if focused {
        return line
            .border_color(rgb(ACCENT))
            .bg(rgba(EDGE))
            .hover_carried();
    }
    line.hover_row().press_row()
}

/// One rail slot: 24px of vertical pitch with the dot centered, and the
/// same hover/pressed language as a row — clicking it is clicking the row.
fn rail_cell(id: (&'static str, usize)) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(24.))
        .w_full()
        .hover_row()
        .press_row()
}

/// The amber `needs you` chip — the exact chip from the Cockpit board's
/// issue-triage cell.
fn needs_you_chip() -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(9.5))
        .text_color(rgb(WAIT))
        .bg(rgba(WAIT_WASH))
        .rounded_sm()
        .px(px(5.))
        .py(px(1.))
        .child("needs you")
}

fn led_color(status: Status) -> u32 {
    match status {
        Status::Streaming => GOOD,
        Status::Blocked => WAIT,
        Status::Closed => FAIL,
        Status::Idle => IDLE,
    }
}

fn dot(size: Pixels, color: u32) -> Div {
    div()
        .flex_shrink_0()
        .w(size)
        .h(size)
        .rounded_full()
        .bg(rgb(color))
}

/// A parked LED: the ring without the fill — present, not running.
fn hollow_dot(size: Pixels) -> Div {
    div()
        .flex_shrink_0()
        .w(size)
        .h(size)
        .rounded_full()
        .border_1()
        .border_color(rgb(INK_FAINT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::ThreadId;
    use gpui::CursorStyle;

    fn row(focused: bool) -> RunningRow {
        RunningRow {
            thread: ThreadId::new(1),
            name: "thread-01".into(),
            binding: "main".into(),
            provider: "cl",
            status: Status::Streaming,
            needs_you: false,
            todos: None,
            focused,
        }
    }

    /// #26: every nav row and rail dot advertises its click with the
    /// pointer cursor — the focused row too, which skips only the wash
    /// (its EDGE ground outranks the hover lift).
    #[test]
    fn nav_rows_carry_the_pointer_cursor_focused_or_not() {
        let cursor = |mut drawn: Stateful<Div>| drawn.style().mouse_cursor;
        assert_eq!(
            cursor(running_row(&row(false))),
            Some(CursorStyle::PointingHand)
        );
        assert_eq!(
            cursor(running_row(&row(true))),
            Some(CursorStyle::PointingHand),
            "the focused row skips the wash, never the cursor"
        );
        assert_eq!(
            cursor(running_dot(&row(false))),
            Some(CursorStyle::PointingHand)
        );
        let parked = ParkedRow {
            thread: ThreadId::new(2),
            name: "thread-02".into(),
            binding: "main".into(),
            provider: "cl",
        };
        assert_eq!(cursor(parked_row(&parked)), Some(CursorStyle::PointingHand));
        assert_eq!(cursor(parked_dot(&parked)), Some(CursorStyle::PointingHand));
    }
}
