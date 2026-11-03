//! Ferrite: streamed words fading in.
//!
//! A view given a [`TextFresh`] watches its text grow: each run of text
//! that appears past what the last frame painted (a streamed chunk) fades in
//! from the view's `ground` over `fade`, by a veil in the ground's colour
//! painted over its glyphs and thinning out on `ease`. Text is counted the
//! way a find counts it (every source run, in paint order, clipped or not),
//! so the view needs nothing from its host but the curve. The first frame
//! only learns where the text ends: nothing fades on first paint. A view
//! drawn without a `TextFresh` forgets what it saw, so its next one starts
//! afresh and any fade still running lands at once. A stream that ended
//! (`land`) speeds what is still fading up to land within a short while,
//! as Claude's streaming text does at turn end ([`fade_progress`]).

use std::{collections::HashMap, ops::Range, rc::Rc, time::Duration, time::Instant};

use gpui::{BorderStyle, Bounds, Edges, Hsla, Pixels, Window, px, quad, transparent_black};

use super::state::TextViewState;

/// How a view's newly streamed text fades in.
#[derive(Clone)]
pub struct TextFresh {
    /// How long a run takes to land.
    pub fade: Duration,
    /// The fade's curve: eased progress for linear progress in `0..=1`.
    pub ease: Rc<dyn Fn(f32) -> f32>,
    /// What the text fades in from: the ground under the view.
    pub ground: Hsla,
    /// The stream ended at this instant: every run still fading lands
    /// within the duration from it (see [`fade_progress`]).
    pub land: Option<(Instant, Duration)>,
}

/// How far (linear, `0..=1`) a fade of `fade` that began at `appeared` is
/// at `now`. Once the stream ends (`land`: when, and within how long), a
/// fade with more left than that speeds up to finish in it — Claude's
/// `playbackRate = remaining / within` — and one with less runs on as it
/// was.
pub fn fade_progress(
    appeared: Instant,
    now: Instant,
    fade: Duration,
    land: Option<(Instant, Duration)>,
) -> f32 {
    let fade = fade.as_secs_f32().max(f32::EPSILON);
    let at = |when: Instant| (when.saturating_duration_since(appeared).as_secs_f32() / fade).clamp(0., 1.);
    let plain = at(now);
    let Some((ended, within)) = land.filter(|(ended, _)| *ended < now && *ended > appeared) else {
        return plain;
    };
    let then = at(ended);
    let left = (1. - then) * fade;
    let within = within.as_secs_f32();
    if left <= within {
        return plain;
    }
    let since = now.saturating_duration_since(ended).as_secs_f32();
    (then + (1. - then) * since / within.max(f32::EPSILON)).clamp(0., 1.)
}

/// Where this frame's runs fall in the view's text, and the runs still
/// fading.
#[derive(Default)]
pub(super) struct FreshTally {
    bases: HashMap<usize, usize>,
    next: usize,
    /// Where the text ended as of the last frame; `None` until a frame has
    /// painted with a `TextFresh`.
    frontier: Option<usize>,
    /// Each run still fading: where it starts in the text, and when it
    /// appeared. A run ends where the next starts (the last at `frontier`).
    runs: Vec<(usize, Instant)>,
}

impl FreshTally {
    /// A frame starts: the runs count afresh in its paint order.
    pub(super) fn begin_frame(&mut self) {
        self.bases.clear();
        self.next = 0;
    }

    /// A frame ends: the text it painted is now seen, and runs that have
    /// landed are dropped. Without `fresh`, everything is forgotten.
    pub(super) fn end_frame(&mut self, fresh: Option<&TextFresh>, now: Instant) {
        let Some(fresh) = fresh else {
            *self = Self::default();
            return;
        };
        // Text can shrink as Markdown closes (`**a` becoming bold `a`).
        self.frontier = Some(self.frontier.map_or(self.next, |end| end.min(self.next)));
        let frontier = self.next;
        self.runs.retain(|(start, at)| {
            *start < frontier && now.saturating_duration_since(*at) < fresh.fade
        });
    }

    /// Where `root`'s text (`len` bytes) starts in the view's text; text
    /// it holds past the last frame's end is a new run, stamped `now`.
    /// `None` while the first frame only learns where the text ends.
    fn place(&mut self, root: usize, len: usize, now: Instant) -> Option<usize> {
        let base = *self.bases.entry(root).or_insert_with(|| {
            let base = self.next;
            self.next += len;
            base
        });
        let frontier = self.frontier?;
        let end = base + len;
        if end > frontier {
            // A run already stamped this frame (an earlier root) just grows.
            if self.runs.last().is_none_or(|(_, at)| *at != now) {
                self.runs.push((frontier, now));
            }
            self.frontier = Some(end);
        }
        Some(base)
    }

    /// Each run still fading, as the range of the view's text it covers and
    /// when it appeared.
    fn fading(&self) -> impl Iterator<Item = (Range<usize>, Instant)> + '_ {
        let frontier = self.frontier.unwrap_or(self.next);
        self.runs.iter().enumerate().map(move |(ix, (start, at))| {
            let stop = self.runs.get(ix + 1).map_or(frontier, |(next, _)| *next);
            (*start..stop, *at)
        })
    }
}

/// Veil the still-fading part of one painted piece of a source run (`root`,
/// `root_len` bytes): the piece covers `own` of the run, and `part` gives
/// where a range of the piece (relative to `own`) painted. Text past the
/// last frame's end is a new run, stamped `now`.
pub(super) fn paint_veil(
    view: &TextViewState,
    root: usize,
    root_len: usize,
    own: Range<usize>,
    part: impl Fn(Range<usize>) -> Vec<Bounds<Pixels>>,
    now: Instant,
    window: &mut Window,
) {
    let Some(fresh) = &view.fresh else {
        return;
    };
    let mut tally = view.fresh_tally.borrow_mut();
    let Some(base) = tally.place(root, root_len, now) else {
        return;
    };
    let piece = base + own.start..base + own.end;
    for (run, at) in tally.fading() {
        let from = run.start.max(piece.start);
        let to = run.end.min(piece.end);
        if from >= to {
            continue;
        }
        let t = fade_progress(at, now, fresh.fade, fresh.land);
        let veil = 1. - (fresh.ease)(t);
        if veil <= 0. {
            continue;
        }
        let ground = Hsla {
            a: fresh.ground.a * veil,
            ..fresh.ground
        };
        for line in part(from - piece.start..to - piece.start) {
            window.paint_quad(quad(
                line,
                px(0.),
                ground,
                Edges::default(),
                transparent_black(),
                BorderStyle::default(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> TextFresh {
        TextFresh {
            fade: Duration::from_millis(300),
            ease: Rc::new(|t| t),
            ground: Hsla::default(),
            land: None,
        }
    }

    /// A fade runs linear to its end; once the stream ends, one with more
    /// than the landing left finishes within it, one with less runs on.
    #[test]
    fn a_fade_lands_within_the_landing_once_the_stream_ends() {
        let ms = Duration::from_millis;
        let start = Instant::now();
        let fade = ms(400);
        assert_eq!(fade_progress(start, start, fade, None), 0.);
        assert!((fade_progress(start, start + ms(100), fade, None) - 0.25).abs() < 1e-4);
        assert_eq!(fade_progress(start, start + ms(500), fade, None), 1.);
        // Ended at 100ms (a quarter in): 300ms left squeezes into 120.
        let land = Some((start + ms(100), ms(120)));
        assert!((fade_progress(start, start + ms(100), fade, land) - 0.25).abs() < 1e-4);
        assert!((fade_progress(start, start + ms(160), fade, land) - 0.625).abs() < 1e-3);
        assert_eq!(fade_progress(start, start + ms(220), fade, land), 1.);
        // Ended at 350ms: 50ms left runs on at its own pace.
        let late = Some((start + ms(350), ms(120)));
        assert!((fade_progress(start, start + ms(375), fade, late) - 0.9375).abs() < 1e-4);
    }

    #[test]
    fn the_first_frame_learns_the_end_and_growth_fades_by_the_run() {
        let fresh = fresh();
        let start = Instant::now();
        let mut tally = FreshTally::default();
        // First paint: two runs of 5 and 3 bytes, nothing fades.
        tally.begin_frame();
        assert_eq!(tally.place(1, 5, start), None);
        assert_eq!(tally.place(2, 3, start), None);
        tally.end_frame(Some(&fresh), start);
        assert_eq!(tally.fading().count(), 0);
        // A chunk grows the second run by 4 and adds a third of 2: one run,
        // from the old end to the new.
        let chunk = start + Duration::from_millis(40);
        tally.begin_frame();
        assert_eq!(tally.place(1, 5, chunk), Some(0));
        assert_eq!(tally.place(2, 7, chunk), Some(5));
        assert_eq!(tally.place(3, 2, chunk), Some(12));
        tally.end_frame(Some(&fresh), chunk);
        assert_eq!(tally.fading().collect::<Vec<_>>(), vec![(8..14, chunk)]);
        // The next chunk is its own run; the first lands after its fade.
        let next = chunk + Duration::from_millis(100);
        tally.begin_frame();
        tally.place(1, 5, next);
        tally.place(2, 7, next);
        tally.place(3, 6, next);
        tally.end_frame(Some(&fresh), next);
        assert_eq!(
            tally.fading().collect::<Vec<_>>(),
            vec![(8..14, chunk), (14..18, next)]
        );
        let later = chunk + Duration::from_millis(320);
        tally.begin_frame();
        tally.place(1, 5, later);
        tally.place(2, 7, later);
        tally.place(3, 6, later);
        tally.end_frame(Some(&fresh), later);
        assert_eq!(tally.fading().collect::<Vec<_>>(), vec![(14..18, next)]);
    }

    #[test]
    fn a_frame_without_fresh_forgets() {
        let fresh = fresh();
        let now = Instant::now();
        let mut tally = FreshTally::default();
        tally.begin_frame();
        tally.place(1, 5, now);
        tally.end_frame(Some(&fresh), now);
        tally.begin_frame();
        tally.place(1, 9, now + Duration::from_millis(10));
        tally.end_frame(None, now);
        tally.begin_frame();
        assert_eq!(tally.place(1, 12, now), None, "it starts afresh");
    }
}
