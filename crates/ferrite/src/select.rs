//! Character-level selection over the transcript (#27). One deep type —
//! `TranscriptSelection` — owns everything between a raw mouse position and
//! the exact text cmd-c copies: the per-frame fragment registry, byte-offset
//! hit-testing, word/line granularity, wash merging, and copy assembly.
//!
//! The cockpit speaks positions (`begin`/`extend`/`release`/`clear`/
//! `copied_text`) and never sees an offset; the pane speaks text runs
//! (`overlay.line`/`piece`) and never sees an anchor. gpui 0.2.2 has no
//! selection element of its own — `TextLayout` gives byte↔pixel mapping and
//! run backgrounds, and everything above that lives here.
//!
//! Endpoints are `(BlockId, ordinal, byte)` — ids, never indices — so
//! eviction cannot slide a selection onto rows the operator never touched.
//! An endpoint whose Block leaves the rendered window clamps the selection
//! to the window start (eviction eats the oldest end first, so everything
//! up to the survivor was swept too); with both ends gone the selection is
//! gone. Copy is what the frame drew: pieces on one visual row join with
//! nothing, rows join with `\n`, and chrome — gutter glyphs, verdict chips,
//! durations, diff line numbers — never registers, so it is never washed
//! and never copied.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use ferrite_core::transcript::{Block, BlockId};
use ferrite_core::ThreadId;
use gpui::{
    point, px, rgba, Bounds, HighlightStyle, Pixels, Point, SharedString, StyledText, TextLayout,
};

use crate::theme::SELECTION;

/// One rendered text run, as the frame registered it: where it came from,
/// what it says, and the layout handle that maps positions to bytes.
/// `layout` is gpui's cloneable Rc handle — layout and prepaint refill it
/// every frame, so a hit test always reads current-frame screen geometry,
/// with no scroll-offset math of our own.
struct Fragment {
    block: BlockId,
    ordinal: u32,
    /// Whether this run starts a new copied line (`overlay.line`) or
    /// continues the current one (`overlay.piece`).
    starts_line: bool,
    text: SharedString,
    layout: TextLayout,
}

/// The per-frame registries, one per Thread, in registration order —
/// which is visual order, because the pane registers runs as it lays them
/// out. Shared between the selection and the overlays it hands out.
type Registry = HashMap<ThreadId, Vec<Fragment>>;

/// One end of the selection, pinned to a rendered run by id.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Caret {
    block: BlockId,
    ordinal: u32,
    byte: usize,
}

/// What one press means, from `MouseDownEvent::click_count`: click-drag
/// characters, double-click words, triple-click (and beyond) whole runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Grain {
    Char,
    Word,
    Line,
}

/// The one live selection: an anchor unit from the press, a head unit
/// under the pointer, both kept at the press's grain so dragging back
/// across the anchor keeps the anchored word or line selected.
struct Live {
    thread: ThreadId,
    grain: Grain,
    anchor: (Caret, Caret),
    head: (Caret, Caret),
    /// A plain press selects nothing until the pointer moves (today's
    /// click rule); word and line presses select their unit at once.
    visible: bool,
    /// The button is still down — only then does `extend` answer.
    gripping: bool,
}

/// The selection endpoints resolved against one frame's rendered window,
/// in visual order — what the overlay's sweep and the copy walk share.
#[derive(Clone, Copy)]
enum Wash {
    None,
    /// An endpoint's Block left the window: everything from the window
    /// start through the survivor (the old clamp-to-start rule).
    FromStart {
        end: Caret,
    },
    Between {
        start: Caret,
        end: Caret,
    },
}

/// Where the wash stands as fragments sweep past in visual order.
enum Sweep {
    Pending,
    Inside,
    Done,
}

#[derive(Default)]
pub struct TranscriptSelection {
    registry: Rc<RefCell<Registry>>,
    live: Option<Live>,
}

impl TranscriptSelection {
    /// A left press on the transcript: anchor at the character under the
    /// pointer, at the grain the click count names. `body` is the
    /// transcript body's window rect — a press outside it (header,
    /// Composer) grips nothing, rather than clamping chrome to the nearest
    /// row; so does one that finds no rendered text (an empty transcript).
    pub fn begin(
        &mut self,
        thread: ThreadId,
        position: Point<Pixels>,
        click_count: usize,
        body: Bounds<Pixels>,
    ) {
        self.live = None;
        if !body.contains(&position) {
            return;
        }
        let registry = self.registry.borrow();
        let Some(fragments) = registry.get(&thread).filter(|list| !list.is_empty()) else {
            return;
        };
        let grain = match click_count {
            0 | 1 => Grain::Char,
            2 => Grain::Word,
            _ => Grain::Line,
        };
        let Some(unit) = unit_at(fragments, position, grain) else {
            return;
        };
        drop(registry);
        self.live = Some(Live {
            thread,
            grain,
            anchor: unit,
            head: unit,
            visible: grain != Grain::Char,
            gripping: true,
        });
    }

    /// The drag's head moved. The position clamps into the origin
    /// transcript's `body` rect, so a sweep that leaves through the
    /// Composer or the Pane's edge selects to the boundary — never into
    /// chrome or a neighbour. True when the selection changed and the
    /// frame should repaint; with no grip standing, nothing changes.
    pub fn extend(
        &mut self,
        thread: ThreadId,
        position: Point<Pixels>,
        body: Bounds<Pixels>,
    ) -> bool {
        let Some(live) = &mut self.live else {
            return false;
        };
        if !live.gripping || live.thread != thread {
            return false;
        }
        if body.size.width < px(2.) || body.size.height < px(2.) {
            return false;
        }
        let position = point(
            position.x.clamp(body.left(), body.right() - px(1.)),
            position.y.clamp(body.top(), body.bottom() - px(1.)),
        );
        let registry = self.registry.borrow();
        let Some(fragments) = registry.get(&thread) else {
            return false;
        };
        let Some(unit) = unit_at(fragments, position, live.grain) else {
            return false;
        };
        if live.visible && live.head == unit {
            return false;
        }
        drop(registry);
        live.head = unit;
        live.visible = true;
        true
    }

    /// The button came up: the drag ends, the selection stays.
    pub fn release(&mut self) {
        if let Some(live) = &mut self.live {
            live.gripping = false;
        }
    }

    /// The Thread a held drag is gripping, visible or not — where the
    /// window's mouse moves aim while the button stays down, wherever the
    /// pointer wanders.
    pub fn gripping_thread(&self) -> Option<ThreadId> {
        self.live
            .as_ref()
            .filter(|live| live.gripping)
            .map(|live| live.thread)
    }

    /// Any press clears the standing selection (and any grip left from a
    /// drag whose release the window never saw). True when something
    /// visible went away and the frame should repaint.
    pub fn clear(&mut self) -> bool {
        self.live.take().is_some_and(|live| live.visible)
    }

    /// The Thread holding a visible selection, for the render heals: a
    /// selection is only real while its rows can be seen.
    pub fn active_thread(&self) -> Option<ThreadId> {
        self.live
            .as_ref()
            .filter(|live| live.visible)
            .map(|live| live.thread)
    }

    /// Drop registries for Threads that no longer have a Pane — parked or
    /// adopted away — so their fragments cannot outlive their rows.
    pub fn retain_threads(&mut self, alive: impl Fn(ThreadId) -> bool) {
        self.registry
            .borrow_mut()
            .retain(|thread, _| alive(*thread));
    }

    /// Exactly the highlighted text, or None with nothing visibly selected
    /// — no invisible clipboard state. Assembled from the registry the
    /// last frame drew: endpoint runs sliced at their bytes, runs between
    /// them whole, pieces of one row joined with nothing, rows with `\n`.
    pub fn copied_text(&self) -> Option<String> {
        let live = self.live.as_ref().filter(|live| live.visible)?;
        let registry = self.registry.borrow();
        let fragments = registry.get(&live.thread)?;
        // Presence is Block-grain, exactly as the overlay resolves it — a
        // caret whose run vanished from a still-rendered Block must read
        // the same here as it paints there (nothing opens, nothing copies),
        // never as an eviction clamp.
        let wash = resolve(live, |caret| {
            fragments
                .iter()
                .position(|fragment| fragment.block == caret.block)
                .map(|at| (at, caret.ordinal as usize))
        });
        let mut sweep = sweep_for(&wash);
        let mut out = String::new();
        for fragment in fragments {
            let Some(range) = advance(
                &mut sweep,
                &wash,
                fragment.block,
                fragment.ordinal,
                &fragment.text,
            ) else {
                continue;
            };
            let slice = &fragment.text[range];
            if slice.is_empty() {
                continue;
            }
            if !out.is_empty() && fragment.starts_line {
                out.push('\n');
            }
            out.push_str(slice);
        }
        (!out.is_empty()).then_some(out)
    }

    /// One frame's render pass over one Pane begins: the Thread's registry
    /// resets (registration order is this frame's visual order), and the
    /// selection resolves against exactly the Blocks this frame will draw
    /// — the rendered tail, not the whole transcript, because copy is
    /// what you see.
    pub fn overlay(&self, thread: ThreadId, rendered: &[Block]) -> SelectionOverlay {
        self.registry.borrow_mut().insert(thread, Vec::new());
        let wash = match &self.live {
            Some(live) if live.thread == thread => resolve(live, |caret| {
                rendered
                    .iter()
                    .position(|block| block.id == caret.block)
                    .map(|at| (at, caret.ordinal as usize))
            }),
            _ => Wash::None,
        };
        SelectionOverlay {
            thread,
            registry: self.registry.clone(),
            sweep: RefCell::new(sweep_for(&wash)),
            wash,
            block: RefCell::new(None),
            next_ordinal: RefCell::new(0),
        }
    }

    /// Every registered run for one Thread, as `(block, ordinal,
    /// starts_line, text)` — the relocated `block_text` exhaustiveness
    /// surface: a Body kind that registers nothing can never be selected
    /// or copied.
    #[cfg(test)]
    pub fn registered(&self, thread: ThreadId) -> Vec<(BlockId, u32, bool, String)> {
        self.registry
            .borrow()
            .get(&thread)
            .map(|fragments| {
                fragments
                    .iter()
                    .map(|fragment| {
                        (
                            fragment.block,
                            fragment.ordinal,
                            fragment.starts_line,
                            fragment.text.to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The window position of a byte inside a registered run, nudged to
    /// the middle of its text line — where a test aims the mouse.
    #[cfg(test)]
    pub fn caret_position(
        &self,
        thread: ThreadId,
        block: BlockId,
        ordinal: u32,
        byte: usize,
    ) -> Option<Point<Pixels>> {
        let registry = self.registry.borrow();
        let fragment = registry
            .get(&thread)?
            .iter()
            .find(|fragment| fragment.block == block && fragment.ordinal == ordinal)?;
        let mut position = fragment.layout.position_for_index(byte)?;
        position.y += fragment.layout.line_height() * 0.5;
        Some(position)
    }
}

/// The selection unit at a window position: the nearest fragment (by row
/// band, then horizontally — gaps and gutters resolve to the run beside
/// them), the byte under the pointer (`index_for_position`'s Err arm is
/// the nearest index, so dragging past an edge clamps for free), widened
/// to the grain.
fn unit_at(
    fragments: &[Fragment],
    position: Point<Pixels>,
    grain: Grain,
) -> Option<(Caret, Caret)> {
    let fragment = nearest(fragments, position)?;
    let byte = match fragment.layout.index_for_position(position) {
        Ok(byte) | Err(byte) => byte,
    };
    let text = &fragment.text;
    let range = match grain {
        Grain::Char => {
            let byte = boundary(text, byte);
            byte..byte
        }
        Grain::Word => word_range(text, byte),
        Grain::Line => 0..text.len(),
    };
    let caret = |byte| Caret {
        block: fragment.block,
        ordinal: fragment.ordinal,
        byte,
    };
    Some((caret(range.start), caret(range.end)))
}

/// The registered run nearest a window position: zero distance wins, then
/// the smallest vertical distance to the run's band, then the smallest
/// horizontal distance — first registered on ties, which is the earlier
/// run in visual order.
fn nearest(fragments: &[Fragment], position: Point<Pixels>) -> Option<&Fragment> {
    let mut best: Option<(Pixels, Pixels, usize)> = None;
    for (at, fragment) in fragments.iter().enumerate() {
        let bounds = fragment.layout.bounds();
        let dy = if position.y < bounds.top() {
            bounds.top() - position.y
        } else if position.y >= bounds.bottom() {
            position.y - bounds.bottom()
        } else {
            Pixels::ZERO
        };
        let dx = if position.x < bounds.left() {
            bounds.left() - position.x
        } else if position.x >= bounds.right() {
            position.x - bounds.right()
        } else {
            Pixels::ZERO
        };
        if best.is_none_or(|(by, bx, _)| (dy, dx) < (by, bx)) {
            best = Some((dy, dx, at));
        }
    }
    best.map(|(_, _, at)| &fragments[at])
}

/// The word under a byte: the contiguous alnum/underscore run, or the one
/// character there when it is not a word character — a double-click on
/// punctuation takes the punctuation, not the neighbouring word.
fn word_range(text: &str, byte: usize) -> Range<usize> {
    if text.is_empty() {
        return 0..0;
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let byte = boundary(text, byte);
    // The character under the caret — at the very end, the one before it.
    let (start, ch) = match text[byte..].chars().next() {
        Some(ch) => (byte, ch),
        None => {
            let ch = text[..byte].chars().next_back().expect("text is non-empty");
            (byte - ch.len_utf8(), ch)
        }
    };
    if !is_word(ch) {
        return start..start + ch.len_utf8();
    }
    let from = text[..start]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map(|(at, _)| at)
        .unwrap_or(start);
    let to = text[start..]
        .char_indices()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map(|(at, c)| start + at + c.len_utf8())
        .unwrap_or(start);
    from..to
}

/// The nearest char boundary at or before `byte` — bytes recorded against
/// one frame's text stay safe against the next frame's.
fn boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Resolve the live endpoints against one visual order. `position_of`
/// places a caret's run in that order, or None when its Block left the
/// rendered window — the eviction clamp: a lone survivor becomes the end
/// and the wash runs from the window start; with every caret gone the
/// selection is gone rather than resurrecting on other rows.
fn resolve(live: &Live, position_of: impl Fn(&Caret) -> Option<(usize, usize)>) -> Wash {
    if !live.visible {
        return Wash::None;
    }
    let carets = [live.anchor.0, live.anchor.1, live.head.0, live.head.1];
    let mut keyed: Vec<((usize, usize, usize), Caret)> = carets
        .iter()
        .filter_map(|caret| {
            position_of(caret).map(|(major, minor)| ((major, minor, caret.byte), *caret))
        })
        .collect();
    if keyed.is_empty() {
        return Wash::None;
    }
    keyed.sort_by_key(|(key, _)| *key);
    let (first_key, first) = keyed[0];
    let (last_key, last) = keyed[keyed.len() - 1];
    if keyed.len() < carets.len() {
        return Wash::FromStart { end: last };
    }
    if first_key == last_key {
        return Wash::None;
    }
    Wash::Between {
        start: first,
        end: last,
    }
}

fn sweep_for(wash: &Wash) -> Sweep {
    match wash {
        Wash::None => Sweep::Done,
        Wash::FromStart { .. } => Sweep::Inside,
        Wash::Between { .. } => Sweep::Pending,
    }
}

/// One run streams past in visual order: the byte range of it the wash
/// covers, or None. The endpoints open and close the sweep by identity —
/// `(BlockId, ordinal)` — so no index bookkeeping survives between frames.
fn advance(
    sweep: &mut Sweep,
    wash: &Wash,
    block: BlockId,
    ordinal: u32,
    text: &str,
) -> Option<Range<usize>> {
    let at = |caret: &Caret| caret.block == block && caret.ordinal == ordinal;
    match sweep {
        Sweep::Done => None,
        Sweep::Inside => {
            let end = match wash {
                Wash::FromStart { end } | Wash::Between { end, .. } => end,
                Wash::None => return None,
            };
            if at(end) {
                *sweep = Sweep::Done;
                Some(0..boundary(text, end.byte))
            } else {
                Some(0..text.len())
            }
        }
        Sweep::Pending => {
            let Wash::Between { start, end } = wash else {
                return None;
            };
            if !at(start) {
                return None;
            }
            if at(end) {
                *sweep = Sweep::Done;
                Some(boundary(text, start.byte)..boundary(text, end.byte))
            } else {
                *sweep = Sweep::Inside;
                Some(boundary(text, start.byte)..text.len())
            }
        }
    }
}

/// One Pane's render-facing seam for one frame. Every text run the pane
/// draws goes through `line` or `piece`: the run registers for hit-testing
/// and copy, and comes back as a `StyledText` wearing the SELECTION wash
/// exactly where the selection covers it. Chrome simply never calls in.
pub struct SelectionOverlay {
    thread: ThreadId,
    registry: Rc<RefCell<Registry>>,
    wash: Wash,
    sweep: RefCell<Sweep>,
    /// Ordinals are per-Block registration order; a Block's runs register
    /// contiguously, so the current Block and one counter are enough.
    block: RefCell<Option<BlockId>>,
    next_ordinal: RefCell<u32>,
}

impl SelectionOverlay {
    /// A run that starts a new copied line.
    pub fn line(
        &self,
        block: BlockId,
        text: impl Into<SharedString>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    ) -> StyledText {
        self.run(block, true, text.into(), highlights)
    }

    /// A run that continues the current line — tool rows compose name,
    /// `(`, summary, `)` as flex pieces, and copy joins them with nothing.
    pub fn piece(
        &self,
        block: BlockId,
        text: impl Into<SharedString>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    ) -> StyledText {
        self.run(block, false, text.into(), highlights)
    }

    fn run(
        &self,
        block: BlockId,
        starts_line: bool,
        text: SharedString,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    ) -> StyledText {
        let mut current = self.block.borrow_mut();
        if *current != Some(block) {
            *current = Some(block);
            *self.next_ordinal.borrow_mut() = 0;
        }
        let ordinal = *self.next_ordinal.borrow();
        *self.next_ordinal.borrow_mut() += 1;
        let wash = advance(
            &mut self.sweep.borrow_mut(),
            &self.wash,
            block,
            ordinal,
            &text,
        );
        let styled = match wash.filter(|range| !range.is_empty()) {
            Some(range) => {
                StyledText::new(text.clone()).with_highlights(washed(&text, highlights, range))
            }
            None => StyledText::new(text.clone()).with_highlights(highlights),
        };
        self.registry
            .borrow_mut()
            .entry(self.thread)
            .or_default()
            .push(Fragment {
                block,
                ordinal,
                starts_line,
                text,
                layout: styled.layout().clone(),
            });
        styled
    }
}

/// The caller's highlight runs, split around the washed range: covered
/// stretches keep their style and gain the SELECTION ground; bare washed
/// stretches gain a ground-only run. `WrappedLine::paint_background` does
/// the painting — the glyphs above are untouched.
fn washed(
    text: &str,
    mut highlights: Vec<(Range<usize>, HighlightStyle)>,
    wash: Range<usize>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    highlights.sort_by_key(|(range, _)| range.start);
    let ground = HighlightStyle {
        background_color: Some(rgba(SELECTION).into()),
        ..Default::default()
    };
    let mut out = Vec::new();
    let mut push = |range: Range<usize>, style: Option<&HighlightStyle>| {
        if range.is_empty() {
            return;
        }
        let covered = range.start.max(wash.start)..range.end.min(wash.end);
        match style {
            Some(style) => {
                if covered.is_empty() {
                    out.push((range, *style));
                    return;
                }
                if range.start < covered.start {
                    out.push((range.start..covered.start, *style));
                }
                let mut washed = *style;
                washed.background_color = ground.background_color;
                out.push((covered.clone(), washed));
                if covered.end < range.end {
                    out.push((covered.end..range.end, *style));
                }
            }
            // A bare stretch only needs a run where the wash covers it.
            None => {
                if !covered.is_empty() {
                    out.push((covered, ground));
                }
            }
        }
    };
    let mut cursor = 0;
    for (range, style) in highlights {
        push(cursor..range.start, None);
        cursor = range.end.max(cursor);
        push(range, Some(&style));
    }
    push(cursor..text.len(), None);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::rgb;

    /// The word grain: alnum/underscore runs, punctuation as itself, and
    /// multibyte text without a panic.
    #[test]
    fn the_word_grain_takes_alnum_runs_and_punctuation_alone() {
        let text = "run cargo_test now";
        assert_eq!(word_range(text, 0), 0..3);
        assert_eq!(word_range(text, 5), 4..14, "underscores join a word");
        assert_eq!(word_range(text, 3), 3..4, "the space is its own unit");
        assert_eq!(word_range(text, 18), 15..18, "the end takes the last word");
        assert_eq!(word_range("héllo wörld", 2), 0..6);
        assert_eq!(word_range("", 3), 0..0);
    }

    /// Bytes recorded against one frame's text stay safe against another:
    /// clamped to length and to char boundaries.
    #[test]
    fn carets_clamp_to_char_boundaries() {
        assert_eq!(boundary("héllo", 2), 1, "inside é steps back");
        assert_eq!(boundary("héllo", 99), 6);
        assert_eq!(boundary("", 4), 0);
    }

    /// Wash merging: covered stretches keep their style and gain the
    /// ground; bare stretches gain ground-only runs; nothing outside the
    /// wash changes.
    #[test]
    fn the_wash_splits_highlight_runs_and_fills_the_gaps() {
        let bold = HighlightStyle {
            color: Some(rgb(crate::theme::INK).into()),
            ..Default::default()
        };
        fn selected(runs: &[(Range<usize>, HighlightStyle)], range: Range<usize>) -> Option<bool> {
            runs.iter()
                .find(|(run, _)| *run == range)
                .map(|(_, style)| style.background_color.is_some())
        }
        // "aaaa BBBB cccc" with BBBB styled, washed 2..12.
        let runs = washed("aaaa BBBB cccc", vec![(5..9, bold)], 2..12);
        assert_eq!(selected(&runs, 2..5), Some(true), "the bare gap before");
        assert_eq!(selected(&runs, 5..9), Some(true), "the styled run, washed");
        assert_eq!(selected(&runs, 9..12), Some(true), "the bare gap after");
        assert!(
            runs.iter()
                .find(|(run, _)| run == &(5..9))
                .is_some_and(|(_, style)| style.color == bold.color),
            "the wash keeps the run's own style"
        );
        assert!(
            !runs.iter().any(|(run, _)| run.start < 2 || run.end > 12),
            "nothing outside the wash grew a run: {runs:?}"
        );

        // A wash cutting into a styled run splits it.
        let runs = washed("aaaa BBBB", vec![(5..9, bold)], 0..7);
        assert_eq!(
            selected(&runs, 5..9),
            None,
            "the run split at the wash edge"
        );
        assert!(
            runs.iter()
                .any(|(run, style)| run == &(5..7) && style.background_color.is_some()),
            "{runs:?}"
        );
        assert!(
            runs.iter()
                .any(|(run, style)| run == &(7..9) && style.background_color.is_none()),
            "{runs:?}"
        );
    }

    fn block(id: u64) -> BlockId {
        // BlockId's constructor is the transcript's; tests mint ids the
        // same way the fold does.
        let mut transcript = ferrite_core::transcript::Transcript::default();
        for _ in 0..id {
            transcript.apply(ferrite_core::transcript::Input::Prompt("x".into()));
        }
        transcript.apply(ferrite_core::transcript::Input::Prompt("x".into()));
        transcript.blocks().last().unwrap().id
    }

    /// A selection assembled by hand (the mouse path needs a window; the
    /// copy path does not): endpoint runs slice at their bytes, middle rows
    /// come whole, pieces join with nothing, rows join with newlines.
    #[test]
    fn copy_slices_endpoints_keeps_middles_and_joins_honestly() {
        let (a, b, c) = (block(1), block(2), block(3));
        let thread = ThreadId::new(7);
        let mut selection = TranscriptSelection::default();
        let overlay = selection.overlay(thread, &[]);
        let _ = overlay.line(a, "alpha line", vec![]);
        let _ = overlay.line(b, "Bash", vec![]);
        let _ = overlay.piece(b, "(", vec![]);
        let _ = overlay.piece(b, "cargo test", vec![]);
        let _ = overlay.piece(b, ")", vec![]);
        let _ = overlay.line(b, "⎿ 42 passed", vec![]);
        let _ = overlay.line(c, "charlie line", vec![]);
        drop(overlay);
        let caret = |block, ordinal, byte| Caret {
            block,
            ordinal,
            byte,
        };
        selection.live = Some(Live {
            thread,
            grain: Grain::Char,
            anchor: (caret(a, 0, 2), caret(a, 0, 2)),
            head: (caret(c, 0, 7), caret(c, 0, 7)),
            visible: true,
            gripping: false,
        });

        assert_eq!(
            selection.copied_text().as_deref(),
            Some("pha line\nBash(cargo test)\n⎿ 42 passed\ncharlie"),
            "endpoints char-grain, middle rows whole, pieces joined bare"
        );

        // Upward drags read the same: the walk orders by the frame, not
        // the drag.
        let live = selection.live.as_mut().unwrap();
        std::mem::swap(&mut live.anchor, &mut live.head);
        assert_eq!(
            selection.copied_text().as_deref(),
            Some("pha line\nBash(cargo test)\n⎿ 42 passed\ncharlie")
        );
    }

    /// The eviction inheritance: an endpoint whose Block left the rendered
    /// window clamps the copy to the window start; with both ends gone the
    /// copy is gone.
    #[test]
    fn an_endpoint_outside_the_window_clamps_and_both_gone_is_gone() {
        let (a, b, c) = (block(1), block(2), block(3));
        let thread = ThreadId::new(7);
        let mut selection = TranscriptSelection::default();
        let caret = |block, byte| Caret {
            block,
            ordinal: 0,
            byte,
        };
        selection.live = Some(Live {
            thread,
            grain: Grain::Char,
            anchor: (caret(a, 2), caret(a, 2)),
            head: (caret(b, 3), caret(b, 3)),
            visible: true,
            gripping: false,
        });

        // The next frame renders without Block a: the survivor is the end,
        // and the wash runs from the window's first row.
        let overlay = selection.overlay(thread, &[]);
        let _ = overlay.line(b, "bravo", vec![]);
        let _ = overlay.line(c, "charlie", vec![]);
        drop(overlay);
        assert_eq!(selection.copied_text().as_deref(), Some("bra"));

        // And with both endpoints outside the window, nothing is copied —
        // never a resurrection on other rows.
        let overlay = selection.overlay(thread, &[]);
        let _ = overlay.line(c, "charlie", vec![]);
        drop(overlay);
        assert_eq!(selection.copied_text(), None);
    }

    /// The overlay and the copy read presence at the same grain: a caret
    /// whose run vanished from a still-rendered Block (a tool row whose
    /// piece list shrank) copies nothing — never an eviction clamp that
    /// paints no wash yet fills the clipboard.
    #[test]
    fn a_vanished_run_on_a_living_block_copies_nothing_rather_than_clamping() {
        let (a, b) = (block(1), block(2));
        let thread = ThreadId::new(7);
        let mut selection = TranscriptSelection::default();
        let caret = |block, ordinal, byte| Caret {
            block,
            ordinal,
            byte,
        };
        // The drag gripped ordinal 1 of Block a — a piece; the next frame
        // renders Block a as a single run.
        selection.live = Some(Live {
            thread,
            grain: Grain::Char,
            anchor: (caret(a, 1, 2), caret(a, 1, 2)),
            head: (caret(b, 0, 3), caret(b, 0, 3)),
            visible: true,
            gripping: false,
        });
        let overlay = selection.overlay(thread, &[]);
        let _ = overlay.line(a, "alpha", vec![]);
        let _ = overlay.line(b, "bravo", vec![]);
        drop(overlay);
        assert_eq!(selection.copied_text(), None);
    }

    /// A collapsed selection — a press that never moved — copies nothing,
    /// so cmd-c leaves the clipboard alone.
    #[test]
    fn a_collapsed_selection_copies_nothing() {
        let a = block(1);
        let thread = ThreadId::new(7);
        let mut selection = TranscriptSelection::default();
        let overlay = selection.overlay(thread, &[]);
        let _ = overlay.line(a, "alpha", vec![]);
        drop(overlay);
        let caret = Caret {
            block: a,
            ordinal: 0,
            byte: 2,
        };
        selection.live = Some(Live {
            thread,
            grain: Grain::Char,
            anchor: (caret, caret),
            head: (caret, caret),
            visible: false,
            gripping: true,
        });
        assert_eq!(selection.copied_text(), None, "an undragged press");

        selection.live.as_mut().unwrap().visible = true;
        assert_eq!(selection.copied_text(), None, "a zero-width drag");
    }
}
