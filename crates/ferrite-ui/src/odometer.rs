//! The working line's token count, ticking as Claude's does: each new
//! reading (`639`, `1.8k`: `progress::token_label`) takes the last one's
//! place, and only the cells that change move — the old glyph fading out
//! `MOTION_COUNT_DRIFT` up while the new one fades in from as far below,
//! over [`motion::COUNT`] (Claude's snap, on the quart: no overshoot). The
//! cells are tabular, so the line never shifts. It is drawn by the loops
//! overlay (`loops_overlay::hosted`): a tick's frames redraw the count and
//! nothing around it. A new turn's smaller count, and every reading under
//! reduced motion, shows at once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use gpui::{canvas, div, prelude::*, px, AnyElement, App, SharedString, Window};

use crate::{components, motion, theme};

/// One cell of the count's face, left to right: what it reads, and what it
/// read before when this tick changed it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cell {
    was: Option<char>,
    now: char,
}

/// `to`'s cells against `from`'s, aligned on their last cell (a count grows
/// at its left): a cell is changed where they differ, or where `from` had
/// none.
fn cells(from: &str, to: &str) -> Vec<Cell> {
    let from: Vec<char> = from.chars().collect();
    let to: Vec<char> = to.chars().collect();
    // `from`'s cell under `to`'s cell `at`, if it had one there.
    let under = |at: usize| (at + from.len()).checked_sub(to.len()).map(|at| from[at]);
    to.iter()
        .enumerate()
        .map(|(at, &now)| Cell {
            was: match under(at) {
                Some(was) if was == now => None,
                Some(was) => Some(was),
                None => Some(' '),
            },
            now,
        })
        .collect()
}

/// A count's last tick: the reading it left, the one it shows, and when.
struct Tick {
    tokens: u64,
    from: String,
    to: String,
    at: Instant,
}

thread_local! {
    /// Each count's tick, by its key: one per working line.
    static TICKS: RefCell<HashMap<SharedString, Tick>> = RefCell::default();
}

/// What the count `key` shows for `tokens` now: its cells, how far its
/// tick has played (1 at rest), and whether it still plays. A new reading
/// ticks from the one before; its first, or a smaller one (a new turn's),
/// lands at once.
fn tick(key: &SharedString, tokens: u64, now: Instant, reduced: bool) -> (Vec<Cell>, f32, bool) {
    let label = ferrite_core::progress::token_label(tokens);
    TICKS.with(|ticks| {
        let mut ticks = ticks.borrow_mut();
        let tick = ticks.entry(key.clone()).or_insert_with(|| Tick {
            tokens,
            from: label.clone(),
            to: label.clone(),
            at: now,
        });
        if tick.to != label {
            tick.from = if tokens < tick.tokens {
                label.clone()
            } else {
                std::mem::take(&mut tick.to)
            };
            tick.to = label.clone();
            tick.at = now;
        }
        tick.tokens = tokens;
        let elapsed = now.saturating_duration_since(tick.at);
        let playing = !reduced && tick.from != tick.to && elapsed < motion::COUNT.duration();
        let t = if playing {
            motion::COUNT.progress_at(elapsed)
        } else {
            1.0
        };
        (cells(&tick.from, &tick.to), t, playing)
    })
}

/// The count `tokens` as its ticking face (`639`, `1.8k`), in the ink and
/// size around it; `key` names the count, so a new reading ticks on from
/// the one before.
pub fn rolling_count(key: impl Into<SharedString>, tokens: u64) -> AnyElement {
    let count = RollingCount {
        key: key.into(),
        tokens,
    };
    crate::loops_overlay::hosted(move || count.clone().into_any_element()).into_any_element()
}

#[derive(IntoElement, Clone)]
struct RollingCount {
    key: SharedString,
    tokens: u64,
}

impl RenderOnce for RollingCount {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let now = cx.background_executor().now();
        let reduced = motion::reduced_motion(cx);
        let (cells, t, playing) = tick(&self.key, self.tokens, now, reduced);
        // Laid out quietly for the overlay, which draws the frames itself.
        let leases = playing && !motion::quiet() && motion::held_loops().is_none();
        let drift = theme::MOTION_COUNT_DRIFT;
        let glyph = |ch: char| SharedString::from(ch.to_string());
        let faces = cells.into_iter().map(move |cell| match cell.was {
            Some(was) if t < 1.0 => div()
                .relative()
                // The new glyph holds the cell's room; both faces drift over it.
                .child(
                    div()
                        .text_color(gpui::transparent_black())
                        .child(glyph(cell.now)),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(-drift * t))
                        .opacity(1.0 - t)
                        .child(glyph(was)),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(drift * (1.0 - t)))
                        .opacity(t)
                        .child(glyph(cell.now)),
                ),
            _ => div().child(glyph(cell.now)),
        });
        components::tabular(
            div()
                .debug_selector(|| "rolling-count".into())
                .relative()
                .flex()
                .flex_shrink_0()
                .children(faces)
                .child(
                    canvas(
                        move |bounds, window, cx| {
                            if leases && window.content_mask().bounds.intersects(&bounds) {
                                motion::lease_frame(window, cx);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(from: &str, to: &str) -> String {
        cells(from, to)
            .iter()
            .map(|cell| if cell.was.is_some() { '^' } else { '.' })
            .collect()
    }

    #[test]
    fn only_the_cells_that_change_move() {
        assert_eq!(changed("639", "640"), ".^^");
        assert_eq!(changed("640", "641"), "..^");
        assert_eq!(changed("1.8k", "1.9k"), "..^.");
        // A new place, or a new shape (`999` → `1.0k`), moves what differs
        // once aligned on the last cell.
        assert_eq!(changed("99", "100"), "^^^");
        assert_eq!(changed("999", "1.0k"), "^^^^");
        assert_eq!(cells("639", "640")[1].was, Some('3'));
    }

    #[test]
    fn a_count_ticks_calmly_and_lands_a_new_turn_at_once() {
        let key = SharedString::from("test-count");
        let start = Instant::now();
        let at = |ms| start + std::time::Duration::from_millis(ms);
        let (_, t, playing) = tick(&key, 639, at(0), false);
        assert_eq!((t, playing), (1.0, false), "the first reading lands");
        let (cells, t, playing) = tick(&key, 640, at(0), false);
        assert!(playing && t == 0.0);
        assert_eq!(cells[2].was, Some('9'));
        let (_, mid, _) = tick(&key, 640, at(theme::MOTION_COUNT_MS / 2), false);
        assert!(
            mid > 0.5 && mid < 1.0,
            "{mid}: the quart is well on by half time"
        );
        let (_, t, playing) = tick(&key, 640, at(theme::MOTION_COUNT_MS), false);
        assert_eq!((t, playing), (1.0, false), "a tick is Claude's snap");
        // A new turn's smaller count lands; reduced motion never ticks.
        let (_, t, playing) = tick(&key, 5, at(200), false);
        assert_eq!((t, playing), (1.0, false));
        let (_, t, playing) = tick(&key, 50, at(210), true);
        assert_eq!((t, playing), (1.0, false));
    }
}
