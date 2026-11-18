//! Find in the transcript (⌘F, `crate::find`).
//!
//! The bar searches everything the conversation says, laid out or not, shown
//! or folded: the texts the rows would draw with every disclosure open (a
//! shut turn's work, a shut group's calls, a call's output, a thought, a
//! long prompt's clamp), each with the disclosures that hide it while shut.
//! Stepping to a hit opens what hides it (`reveal_found`), and the row it
//! then draws on is brought in by scrolling to it; once its text has
//! painted, the active occurrence reports where (`FindReport::revealed`) and
//! the view settles it a third of the way down, clear of the pinned band.

use std::collections::HashMap;

use gpui::{prelude::*, AnyElement, Context, Window};

use super::{Navigate, RowShape, TranscriptRows, TranscriptView};
use crate::pane::{DisclosureId, DisclosureState};
use crate::select::{FindMarks, RunText};

/// The bar's count, its hit's row, and where that occurrence last painted.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct FindState {
    pub count: Option<gpui::SharedString>,
    pub row: Option<usize>,
    pub painted: Option<gpui::Bounds<gpui::Pixels>>,
}

/// Where an occurrence a host waits on has painted: it waits `reveal`
/// frames for it (a hit in text that is not drawn never reports), and 0 when
/// none is due.
pub(super) fn painted(
    reveal: &mut u8,
    report: &gpui::base::text::FindReport,
    window: &mut Window,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    if *reveal == 0 {
        return None;
    }
    let found = report.revealed.get();
    if found.is_some() {
        *reveal = 0;
    } else {
        *reveal -= 1;
        if *reveal > 0 {
            window.request_animation_frame();
        }
    }
    found
}

/// The open bar and what it needs between frames.
pub(super) struct TranscriptFind {
    bar: crate::find::Find,
    /// What the bar searches, in reading order (`TranscriptView::find_units`).
    units: Vec<FindUnit>,
    /// The units are out of date: the blocks changed since they were read.
    pub(super) stale: bool,
    /// Disclosures a step has asked to open and the Pane has not yet opened:
    /// a toggle is never asked twice.
    opening: Vec<DisclosureId>,
    /// Frames left to bring the active hit into view; 0 when none is due.
    reveal: u8,
}

/// One text the bar searches: as drawn open, and as a find counts in it
/// (`RunText::find_text`). A text whose source is unchanged keeps its
/// rendering, so a keystroke parses only what changed.
struct FindUnit {
    text: RunText,
    rendered: String,
}

impl TranscriptView {
    /// Open the bar (or take the keyboard back to it).
    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() {
            self.find = Some(TranscriptFind {
                bar: crate::find::Find::new(cx, |view: &mut Self, cx| view.refind(cx)),
                units: Vec::new(),
                stale: true,
                opening: Vec::new(),
                reveal: 0,
            });
            cx.notify();
        }
        if let Some(find) = &self.find {
            find.bar.focus(window, cx);
        }
    }

    /// Put `text` on the open bar's line, as typed (a reference scene).
    #[cfg(feature = "visual-reference")]
    pub fn type_find(&self, text: &str, cx: &mut gpui::App) {
        if let Some(find) = &self.find {
            find.bar.type_query(text, cx);
        }
    }

    pub(crate) fn close_find(&mut self, cx: &mut Context<Self>) {
        if self.find.take().is_some() {
            cx.notify();
        }
    }

    /// Whether the bar is open over the transcript's top right.
    pub fn find_open(&self) -> bool {
        self.find.is_some()
    }

    /// Search again: the line changed, or the texts under it did. A new
    /// query is brought into view; the same query keeps its place.
    pub(super) fn refind(&mut self, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_ref() else {
            return;
        };
        // The hit the bar is on, by its text's key: blocks trimmed or added
        // ahead of it must not move the bar to another occurrence.
        let on = find
            .bar
            .active()
            .and_then(|hit| Some((find.units.get(hit.unit)?.text.key.clone(), hit.ordinal)));
        if find.stale {
            let units = self.find_units();
            if let Some(find) = self.find.as_mut() {
                find.units = units;
                find.stale = false;
            }
        }
        let first_shown = (0..self.rows.len())
            .find(|&row| self.scroll.item_is_visible(row))
            .unwrap_or(0);
        let first_text = self
            .texts
            .iter()
            .find(|(row, _)| *row >= first_shown)
            .map(|(_, text)| text.key.clone());
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let from = first_text
            .and_then(|key| find.units.iter().position(|unit| unit.text.key == key))
            .unwrap_or(0);
        let TranscriptFind { bar, units, .. } = find;
        let changed = bar.search(units.iter().map(|unit| unit.rendered.as_str()), from, cx);
        if changed {
            self.reveal_found(cx);
        } else if let Some((key, ordinal)) = on {
            if let Some(unit) = units.iter().position(|unit| unit.text.key == key) {
                bar.keep(crate::find::Hit { unit, ordinal });
            }
        }
        if self
            .find
            .as_ref()
            .is_some_and(|find| !find.opening.is_empty())
        {
            self.reveal_found(cx);
        }
        cx.notify();
    }

    /// Everything the transcript says, in reading order: the rows with every
    /// fold, group, call, thought and clamp open, each text with what hides
    /// it. Its keys are those the shut rows draw, so a hit already in view
    /// is found by its key.
    fn find_units(&self) -> Vec<FindUnit> {
        let shape: RowShape = self.input.shape();
        let open = TranscriptRows::new(
            &self.input.body.blocks,
            &shape.opened(&self.input.body.blocks),
            self.grid().size,
        );
        let selection = crate::select::TranscriptText::default()
            .overlay_scoped(
                self.input.presentation.thread,
                self.input.presentation.namespace.clone(),
                &self.input.body.blocks,
                self.rich.clone(),
            )
            .with_hidden(super::rows::hidden_by(&self.input.body.blocks, &shape));
        let (_, texts) = selection.capture_members(|| {
            for row in open.rows() {
                selection.begin_row();
                self.collect_row_text(row, &selection, true);
            }
        });
        let mut old: HashMap<_, _> = self
            .find
            .iter()
            .flat_map(|find| &find.units)
            .map(|unit| {
                (
                    unit.text.key.clone(),
                    (unit.text.source.clone(), unit.rendered.clone()),
                )
            })
            .collect();
        texts
            .into_iter()
            .map(|text| {
                let rendered = match old.remove(&text.key) {
                    Some((source, rendered)) if source == text.source => rendered,
                    _ => text.find_text(),
                };
                FindUnit { text, rendered }
            })
            .collect()
    }

    /// ⏎ / ⇧⏎: the next occurrence (or the previous), wrapping.
    pub(super) fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(find) = self.find.as_mut() {
            find.opening.clear();
        }
        if self
            .find
            .as_mut()
            .is_some_and(|find| find.bar.step(forward).is_some())
        {
            self.reveal_found(cx);
            cx.notify();
        }
    }

    /// Bring the active hit into view: open what hides it (the Pane opens
    /// each and syncs the rows, which asks again), then scroll to its row
    /// and settle its occurrence once it paints (`settle_found`).
    fn reveal_found(&mut self, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_ref() else {
            return;
        };
        let Some(unit) = find.bar.active().and_then(|hit| find.units.get(hit.unit)) else {
            return;
        };
        let key = unit.text.key.clone();
        let shut: Vec<DisclosureId> = unit
            .text
            .reveals
            .iter()
            .filter(|id| self.tool_state(*id) != DisclosureState::Expanded)
            .cloned()
            .collect();
        if !shut.is_empty() {
            let find = self.find.as_mut().expect("the bar is open");
            let ask: Vec<_> = shut
                .into_iter()
                .filter(|id| !find.opening.contains(id))
                .collect();
            find.opening.extend(ask.iter().cloned());
            for id in ask {
                self.toggle_disclosure(id, cx);
            }
            return;
        }
        let Some(&(row, _)) = self.texts.iter().find(|(_, text)| text.key == key) else {
            return;
        };
        let find = self.find.as_mut().expect("the bar is open");
        find.opening.clear();
        find.bar.report().revealed.set(None);
        self.navigate(Navigate::Reveal { row }, cx);
        if let Some(find) = self.find.as_mut() {
            find.reveal = crate::find::REVEAL_FRAMES;
        }
    }

    /// The reader took the scroll: the settle that was due no longer is.
    pub(super) fn drop_pending_reveal(&mut self) {
        if let Some(find) = self.find.as_mut() {
            find.reveal = 0;
        }
    }

    /// Each render while a reveal is due: once the active occurrence has
    /// painted, scroll it a third of the way down the view if it lies
    /// outside it (or under the pinned band).
    pub(super) fn settle_found(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let found = painted(&mut find.reveal, find.bar.report(), window);
        if let Some(found) = found {
            self.settle_hit(found, cx);
        }
    }

    /// Scroll an occurrence that painted at `found` to the reading line, if
    /// it lies outside the view or under the pinned band.
    pub(super) fn settle_hit(&mut self, found: gpui::Bounds<gpui::Pixels>, cx: &mut Context<Self>) {
        let viewport = self.scroll.list_state().viewport_bounds();
        let top = viewport.top() + self.scroll.top_mask();
        if found.top() < top || found.bottom() > viewport.bottom() {
            self.navigate(Navigate::Settle { at: found.top() }, cx);
        }
    }

    /// What the runs mark: the query, and the occurrence the bar is on.
    pub(super) fn find_marks(&self) -> Option<FindMarks> {
        let Some(find) = self.find.as_ref() else {
            return self.cited.as_ref().map(|cited| cited.marks.clone());
        };
        Some(FindMarks {
            query: find.bar.query().clone(),
            active: find.bar.active().and_then(|hit| {
                let unit = find.units.get(hit.unit)?;
                Some((unit.text.key.clone(), hit.ordinal))
            }),
            report: find.bar.report().clone(),
        })
    }

    /// The bar over the transcript (`Find::bar`).
    pub(super) fn find_bar(&self) -> Option<AnyElement> {
        Some(self.find.as_ref()?.bar.bar().into_any_element())
    }

    /// What the bar reads and where its hit is — what the tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn find_state(&self) -> Option<FindState> {
        let find = self.find.as_ref()?;
        Some(FindState {
            count: find.bar.count(),
            row: find
                .bar
                .active()
                .and_then(|hit| find.units.get(hit.unit))
                .and_then(|unit| {
                    self.texts
                        .iter()
                        .find(|(_, text)| text.key == unit.text.key)
                })
                .map(|(row, _)| *row),
            painted: find.bar.report().revealed.get(),
        })
    }
}
