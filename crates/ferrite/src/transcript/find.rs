//! Find in the transcript (⌘F, `crate::find`).
//!
//! The bar searches every row this transcript holds, laid out or not: the
//! texts copy reads (`TextRuns::capture_members`), each with its row, as
//! drawn — a folded call's output is not searched, as it is not shown. A
//! hit off screen is brought in by scrolling to its row; once its text has
//! painted, the active occurrence reports where (`FindReport::revealed`) and
//! the view settles it a third of the way down, clear of the pinned band.

use std::collections::HashMap;

use gpui::{prelude::*, px, AnyElement, Context, SharedString, Window};

use super::{Runway, TranscriptView};
use crate::select::FindMarks;

/// The bar's count, its hit's row, and where that occurrence last painted.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct FindState {
    pub count: Option<SharedString>,
    pub row: Option<usize>,
    pub painted: Option<gpui::Bounds<gpui::Pixels>>,
}

/// The open bar and what it needs between frames.
pub(super) struct TranscriptFind {
    bar: crate::find::Find,
    /// Each text as a find counts in it (`RunText::find_text`), by key,
    /// kept while its source is unchanged: a keystroke parses only what
    /// changed.
    plain: HashMap<SharedString, (SharedString, String)>,
    /// Frames left to bring the active hit into view; 0 when none is due.
    reveal: u8,
}

impl TranscriptView {
    /// Open the bar (or take the keyboard back to it).
    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() {
            self.find = Some(TranscriptFind {
                bar: crate::find::Find::new(cx, |view: &mut Self, cx| view.refind(cx)),
                plain: HashMap::new(),
                reveal: 0,
            });
            cx.notify();
        }
        if let Some(find) = &self.find {
            find.bar.focus(window, cx);
        }
    }

    pub(crate) fn close_find(&mut self, cx: &mut Context<Self>) {
        if self.find.take().is_some() {
            cx.notify();
        }
    }

    /// Whether the bar's line holds the keyboard.
    pub(crate) fn find_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.find
            .as_ref()
            .is_some_and(|find| find.bar.holds_focus(window, cx))
    }

    /// Search again: the line changed, or the texts under it did. A new
    /// query is brought into view; the same query keeps its place.
    pub(super) fn refind(&mut self, cx: &mut Context<Self>) {
        let first_shown = (0..self.rows.len())
            .find(|&row| self.scroll.item_is_visible(row))
            .unwrap_or(0);
        let from = self
            .texts
            .iter()
            .position(|(row, _)| *row >= first_shown)
            .unwrap_or(0);
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let mut old = std::mem::take(&mut find.plain);
        find.plain = self
            .texts
            .iter()
            .map(|(_, text)| {
                let rendered = match old.remove(&text.key) {
                    Some((source, rendered)) if source == text.source => rendered,
                    _ => text.find_text(),
                };
                (text.key.clone(), (text.source.clone(), rendered))
            })
            .collect();
        let TranscriptFind { bar, plain, .. } = find;
        let units = self
            .texts
            .iter()
            .map(|(_, text)| plain[&text.key].1.as_str());
        let changed = bar.search(units, from, cx);
        if changed {
            self.reveal_found();
        }
        cx.notify();
    }

    /// ⏎ / ⇧⏎: the next occurrence (or the previous), wrapping.
    pub(super) fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self
            .find
            .as_mut()
            .is_some_and(|find| find.bar.step(forward).is_some())
        {
            self.reveal_found();
            cx.notify();
        }
    }

    /// Bring the active hit's row into view, and settle its occurrence
    /// once it paints (`settle_found`).
    fn reveal_found(&mut self) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let Some(&(row, _)) = find.bar.active().and_then(|hit| self.texts.get(hit.unit)) else {
            return;
        };
        find.bar.report().revealed.set(None);
        find.reveal = crate::find::REVEAL_FRAMES;
        if !self.scroll.item_is_visible(row) {
            self.glide = None;
            self.runway.set(Runway::default());
            self.scroll.scroll_to(row, px(0.));
        }
    }

    /// Each render while a reveal is due: once the active occurrence has
    /// painted, scroll it a third of the way down the view if it lies
    /// outside it (or under the pinned band).
    pub(super) fn settle_found(&mut self, window: &mut Window) {
        let Some(find) = self.find.as_mut().filter(|find| find.reveal > 0) else {
            return;
        };
        let Some(found) = find.bar.report().revealed.get() else {
            find.reveal -= 1;
            if find.reveal > 0 {
                window.request_animation_frame();
            }
            return;
        };
        find.reveal = 0;
        let viewport = self.scroll.list_state().viewport_bounds();
        let top = viewport.top() + self.scroll.top_mask();
        if found.top() < top || found.bottom() > viewport.bottom() {
            let goal = top + (viewport.bottom() - top) / 3.;
            self.scroll.list_state().scroll_by(found.top() - goal);
        }
    }

    /// What the runs mark: the query, and the occurrence the bar is on.
    pub(super) fn find_marks(&self) -> Option<FindMarks> {
        let find = self.find.as_ref()?;
        Some(FindMarks {
            query: find.bar.query().clone(),
            active: find.bar.active().and_then(|hit| {
                let (_, text) = self.texts.get(hit.unit)?;
                Some((text.key.clone(), hit.ordinal))
            }),
            report: find.bar.report().clone(),
        })
    }

    /// The bar over the transcript (`Find::bar`).
    pub(super) fn find_bar(&self) -> Option<AnyElement> {
        Some(self.find.as_ref()?.bar.bar().into_any_element())
    }

    /// What the bar reads and where its hit is — what the tests read.
    #[cfg(test)]
    pub(crate) fn find_state(&self) -> Option<FindState> {
        let find = self.find.as_ref()?;
        Some(FindState {
            count: find.bar.count(),
            row: find
                .bar
                .active()
                .and_then(|hit| self.texts.get(hit.unit))
                .map(|(row, _)| *row),
            painted: find.bar.report().revealed.get(),
        })
    }
}
