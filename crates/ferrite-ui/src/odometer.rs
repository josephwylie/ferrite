//! The working line's token count, rolling: Claude Code's climbing count
//! drawn the way a dial does. The count eases to each new value over
//! [`motion::COUNT`]; its digits sit in tabular cells, each rolling up
//! from one digit to the next exactly as an odometer's wheel does — the
//! ones turning with the value, a higher wheel only while every wheel
//! under it carries — so the line never shifts. It is drawn by the loops
//! overlay (`loops_overlay::hosted`): a roll's frames redraw the count and
//! nothing around it. Reduced motion shows each value at once.

use std::cell::RefCell;
use std::collections::HashMap;

use gpui::{canvas, div, prelude::*, AnyElement, App, SharedString, Window};

use crate::{components, motion};

/// One cell of the count's face, left to right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Glyph {
    /// A mark that never rolls: the decimal point, the `k`.
    Mark(char),
    /// A digit wheel: `now` (`None` for a blank leading cell) leaving
    /// upwards as `next` comes in from below, `t` of the way.
    Digit { now: Option<u8>, next: u8, t: f32 },
}

/// The count's label at rest (`639`, `1.8k`, `12k`: `progress::token_label`)
/// and what one unit of its last digit is worth in tokens.
fn label_of(tokens: u64) -> (String, u64) {
    let label = ferrite_core::progress::token_label(tokens);
    let unit = match (label.ends_with('k'), label.contains('.')) {
        (true, true) => 100,
        (true, false) => 1000,
        _ => 1,
    };
    (label, unit)
}

/// The value the count rests at for `tokens`: what its label says, in
/// tokens (`1.8k` → 1800), so a roll lands with every wheel square.
pub fn rest(tokens: u64) -> u64 {
    let (label, unit) = label_of(tokens);
    let digits: String = label.chars().filter(char::is_ascii_digit).collect();
    digits.parse::<u64>().unwrap_or(0) * unit
}

/// The face of a count heading for `target`, showing `shown` tokens on
/// the way: the target's label, cell for cell, each digit's wheel where
/// `shown` has turned it. At `shown == rest(target)` it reads the label.
pub fn face(target: u64, shown: f64) -> Vec<Glyph> {
    let (label, unit) = label_of(target);
    let value = (shown / unit as f64).max(0.0);
    let decimals = label.split_once('.').map_or(0, |(_, after)| {
        after.chars().filter(char::is_ascii_digit).count()
    }) as u32;
    let mut place = label.chars().filter(char::is_ascii_digit).count() as u32;
    label
        .chars()
        .map(|ch| {
            if ch.is_ascii_digit() {
                place -= 1;
                wheel(value, place, decimals)
            } else {
                Glyph::Mark(ch)
            }
        })
        .collect()
}

/// The wheel at `place` (0 the last digit) of a dial showing `value`: it
/// turns with the value's fraction at the last place, and above it only
/// while every wheel under it passes from 9 to 0. A leading wheel still at
/// 0 is blank, unless it is the units before the point.
fn wheel(value: f64, place: u32, decimals: u32) -> Glyph {
    let scale = 10f64.powi(place as i32);
    let whole = (value / scale).floor();
    let below = value - whole * scale;
    let t = if place == 0 {
        below
    } else {
        (below - (scale - 1.0)).max(0.0)
    };
    let digit = (whole % 10.0) as u8;
    let blank = whole == 0.0 && place > decimals;
    Glyph::Digit {
        now: (!blank).then_some(digit),
        next: ((whole as u64 + 1) % 10) as u8,
        t: t as f32,
    }
}

thread_local! {
    /// Each count's tween, by its key: one per working line.
    static ROLLS: RefCell<HashMap<SharedString, motion::Tween>> = RefCell::default();
}

/// What the count `key` shows now on its way to `to` (a rest value), and
/// whether it is still rolling. A new value rolls from wherever the count
/// stands; its first, or a smaller one (a new turn's), lands at once.
fn roll(key: &SharedString, to: u64, now: std::time::Instant, reduced: bool) -> (f64, bool) {
    ROLLS.with(|rolls| {
        let mut rolls = rolls.borrow_mut();
        let to = to as f32;
        let tween = rolls
            .entry(key.clone())
            .or_insert_with(|| motion::Tween::new(to, to, motion::COUNT, now));
        if tween.to != to {
            *tween = if to < tween.to {
                motion::Tween::new(to, to, motion::COUNT, now)
            } else {
                motion::Tween::retarget(Some(*tween), to, to, motion::COUNT, now, reduced)
            };
        }
        (
            f64::from(tween.value(now, reduced)),
            tween.running(now, reduced),
        )
    })
}

/// The count `tokens` as its rolling face (`639`, `1.8k`), in the ink and
/// size around it; `key` names the count, so a new value rolls on from the
/// one before.
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let now = cx.background_executor().now();
        let reduced = motion::reduced_motion(cx);
        let (shown, rolling) = roll(&self.key, rest(self.tokens), now, reduced);
        // Laid out quietly for the overlay, which draws the frames itself.
        let leases = rolling && !motion::quiet() && motion::held_loops().is_none();
        let line = window.text_style().line_height_in_pixels(window.rem_size());
        let digit = |digit: u8| SharedString::from(char::from(b'0' + digit).to_string());
        let cells = face(self.tokens, shown)
            .into_iter()
            .map(move |glyph| match glyph {
                Glyph::Mark(mark) => div().child(SharedString::from(mark.to_string())),
                // A place the count has not reached takes no room until its
                // wheel starts to turn in.
                Glyph::Digit { now: None, t, .. } if t == 0.0 => div(),
                Glyph::Digit { now, next, t } => div()
                    .relative()
                    .overflow_hidden()
                    // A tabular `0` holds the cell's width; the wheel turns over it.
                    .child(div().text_color(gpui::transparent_black()).child("0"))
                    .children(
                        now.map(|now| div().absolute().left_0().top(line * -t).child(digit(now))),
                    )
                    .when(t > 0.0, |cell| {
                        cell.child(
                            div()
                                .absolute()
                                .left_0()
                                .top(line * (1.0 - t))
                                .child(digit(next)),
                        )
                    }),
            });
        components::tabular(
            div()
                .debug_selector(|| "rolling-count".into())
                .relative()
                .flex()
                .flex_shrink_0()
                .children(cells)
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

    /// The face's words: each wheel's digit while it rests.
    fn read(glyphs: &[Glyph]) -> String {
        glyphs
            .iter()
            .map(|glyph| match glyph {
                Glyph::Mark(mark) => *mark,
                Glyph::Digit { now: Some(d), .. } => char::from(b'0' + d),
                Glyph::Digit { now: None, .. } => ' ',
            })
            .collect()
    }

    #[test]
    fn at_rest_the_face_reads_the_label() {
        for tokens in [
            0, 7, 39, 639, 999, 1_000, 1_849, 9_949, 9_960, 12_400, 120_000,
        ] {
            let face = face(tokens, rest(tokens) as f64);
            let label = ferrite_core::progress::token_label(tokens);
            assert_eq!(read(&face), label, "{tokens}");
            assert!(
                face.iter()
                    .all(|glyph| !matches!(glyph, Glyph::Digit { t, .. } if *t != 0.0)),
                "{tokens} rests square"
            );
        }
        assert_eq!(rest(1_849), 1_800);
        assert_eq!(rest(12_400), 12_000);
    }

    #[test]
    fn only_the_wheels_that_carry_turn() {
        // 639 → 640, a quarter of the way: the ones turn 9 → 0 and the tens
        // carry 3 → 4 with them; the hundreds stand.
        let face = face(640, 639.25);
        assert_eq!(
            face,
            [
                Glyph::Digit {
                    now: Some(6),
                    next: 7,
                    t: 0.0
                },
                Glyph::Digit {
                    now: Some(3),
                    next: 4,
                    t: 0.25
                },
                Glyph::Digit {
                    now: Some(9),
                    next: 0,
                    t: 0.25
                },
            ]
        );
        // 631.5 on the way to 640: only the ones turn.
        let face = super::face(640, 631.5);
        assert!(matches!(face[1], Glyph::Digit { now: Some(3), t, .. } if t == 0.0));
        assert!(matches!(face[2], Glyph::Digit { now: Some(1), t, .. } if t == 0.5));
    }

    #[test]
    fn a_new_place_rolls_in_from_blank() {
        // 99 → 100: the hundreds wheel comes in from blank.
        let face = face(100, 99.5);
        assert_eq!(
            face[0],
            Glyph::Digit {
                now: None,
                next: 1,
                t: 0.5
            }
        );
        // 0.9k → 1.0k: the units before the point show 0, never blank.
        let face = super::face(1_000, 950.0);
        assert_eq!(read(&face), "0.9k");
    }

    #[test]
    fn a_count_rolls_up_and_lands_a_new_turn_at_once() {
        let key = SharedString::from("test-count");
        let start = std::time::Instant::now();
        let at = |ms| start + std::time::Duration::from_millis(ms);
        assert_eq!(roll(&key, 100, at(0), false), (100.0, false));
        let (shown, rolling) = roll(&key, 200, at(0), false);
        assert_eq!((shown, rolling), (100.0, true));
        let (mid, _) = roll(&key, 200, at(150), false);
        assert!(
            mid > 150.0 && mid < 200.0,
            "{mid}: the quart is well on by half time"
        );
        assert_eq!(roll(&key, 200, at(300), false), (200.0, false));
        // Retargeted mid-roll, it goes on from where it stands.
        roll(&key, 300, at(400), false);
        let (on, _) = roll(&key, 400, at(450), false);
        assert!(on > 200.0 && on < 400.0, "{on}");
        // A new turn's smaller count lands; reduced motion never rolls.
        assert_eq!(roll(&key, 5, at(460), false), (5.0, false));
        assert_eq!(roll(&key, 50, at(470), true), (50.0, false));
    }
}
