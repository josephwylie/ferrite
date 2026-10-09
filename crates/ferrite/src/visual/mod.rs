//! Visuals: an agent's interactive HTML (a chart, a mock, a diagram) drawn
//! inline in the transcript (ADR 0013).
//!
//! The page is rendered off-screen by a web engine and drawn as an ordinary
//! GPUI image wherever the transcript's layout puts it, so scrolling, clipping
//! and popovers need no coordination with a native child view.
//!
//! What the rest of the app sees:
//!
//! - [`boot`], first thing in `main` (a Chromium helper process exits there),
//!   and [`init`] once GPUI is up. Chromium itself starts on the first visual.
//! - [`Shelf`]: one per transcript. It draws a Visual row ([`Shelf::row`]),
//!   keeps each visual's live page near the screen and lets the far ones go
//!   ([`Shelf::reach`]), and forgets visuals that left the transcript.
//! - [`collect_text`]: a Visual row's selectable text.
//! - [`renderer`]: the screenshot an agent gets back, for core's
//!   `VisualRenderer` seam.
//!
//! Inside: the engine seam ([`web`]: `Engine`/`View`, the Chromium adapter on
//! macOS and a fake), the element that draws a view ([`element::WebView`]),
//! the theme tokens a page is styled with ([`tokens`]), the engine's lazy
//! lifecycle ([`engine`]) and the row's look ([`card`]).

mod card;
pub(crate) mod element;
pub(crate) mod engine;
pub(crate) mod renderer;
mod shelf;
#[cfg(feature = "visual-reference")]
pub(crate) mod shots;
pub(crate) mod tokens;
pub(crate) mod web;

pub(crate) use card::collect_text;
pub(crate) use engine::{boot, init};
pub(crate) use ferrite_core::visual::{Status as VisualStatus, Visual};
pub(crate) use shelf::Shelf;

use ferrite_core::transcript::{Block, Body};

/// The visual a block holds, if it is one.
pub(crate) fn visual_of(block: &Block) -> Option<&Visual> {
    match &block.body {
        Body::Visual(visual) => Some(visual),
        _ => None,
    }
}
