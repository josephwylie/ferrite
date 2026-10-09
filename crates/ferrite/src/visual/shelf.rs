//! One transcript's visuals: each visual's live page, kept live while it is
//! on or near the screen and let go when far, and the row that shows it.
//!
//! The transcript holds a [`Shelf`] and asks three things of it:
//!
//! - [`Shelf::row`]: a Visual row, drawn (the page opens on first draw, or
//!   earlier when near);
//! - [`Shelf::reach`]: after each layout, where the visual rows are: views
//!   within [`NEAR`] screens open (at the width they will have, so scrolling
//!   in costs nothing) or stay live; beyond [`FAR`] and undrawn they're
//!   hidden, and the engine releases them after its grace period. The last
//!   frame stays and is drawn whenever the row is (a frozen frame), so a far
//!   view coming back shows no placeholder;
//! - [`Shelf::retain`]: forget visuals no longer in the transcript.
//!
//! A page whose height changes calls the transcript back (`on_height`) so the
//! list re-measures that row, off-screen if need be, never under the reader.
//! The theme follows the reading size: every draw passes the current tokens,
//! and a change restyles live pages in place.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ferrite_core::settings::ReadingSize;
use ferrite_core::transcript::BlockId;
use gpui::{px, AnyElement, App, ClipboardItem, Entity, ListState, Pixels, Subscription, Window};

use super::card::{self, Actions, Stage, MIN_LIVE_WIDTH};
use super::element::{HeightChanged, Reach, WebView};
use super::engine::{Availability, Visuals};
use super::tokens::{self, Scheme};
use super::web::Page;
use super::{Visual, VisualStatus};
use crate::pane::Grid;
use crate::select::TextRuns;

/// Keep views live within this many viewport heights of the screen: they open
/// (or reload) here, before they scroll in.
pub(crate) const NEAR: f32 = 1.5;
/// ...and let them go beyond this many. The gap keeps a row at the edge from
/// flapping; the engine's grace period then delays the release.
pub(crate) const FAR: f32 = 2.5;
/// Height assumed for a row the list has never measured: low on purpose, so
/// an estimate only ever opens a view early.
const ROW_ESTIMATE: f32 = 20.;

/// Called with a visual's id when its page's height changed.
pub(crate) type OnHeight = Rc<dyn Fn(&str, &mut App)>;

/// See the module docs. Cheap to clone (shared state).
#[derive(Clone)]
pub(crate) struct Shelf {
    inner: Rc<RefCell<Inner>>,
    on_height: OnHeight,
}

struct Inner {
    slots: HashMap<String, Slot>,
}

struct Slot {
    web: Entity<WebView>,
    near: bool,
    _height: Subscription,
}

/// The tokens a page gets at `reading`.
fn theme_css(reading: ReadingSize) -> String {
    tokens::theme_css(Scheme::APP, reading)
}

impl Shelf {
    pub(crate) fn new(on_height: OnHeight) -> Self {
        Self { inner: Rc::new(RefCell::new(Inner { slots: HashMap::new() })), on_height }
    }

    /// The live view for `visual`, opened (unpainted) if it has none, and
    /// brought up to date with its HTML and the theme. `None` while there is
    /// nothing to show (no engine yet, no HTML yet).
    fn view(&self, visual: &Visual, reading: ReadingSize, cx: &mut App) -> Option<Entity<WebView>> {
        if visual.html.trim().is_empty() {
            return None;
        }
        let engine = match Visuals::engine(cx) {
            Availability::Ready(engine) => engine,
            Availability::Starting | Availability::Unavailable(_) => return None,
        };
        let theme = theme_css(reading);
        let existing = self.inner.borrow().slots.get(&visual.id).map(|slot| slot.web.clone());
        if let Some(web) = existing {
            web.update(cx, |web, cx| {
                web.set_html(visual.html.clone(), cx);
                web.set_theme(theme, cx);
            });
            return Some(web);
        }
        let web = WebView::new(engine, Page { html: visual.html.clone(), theme_css: theme }, cx);
        let on_height = self.on_height.clone();
        let id = visual.id.clone();
        let subscription = cx.subscribe(&web, move |_, _: &HeightChanged, cx| on_height(&id, cx));
        self.inner.borrow_mut().slots.insert(
            visual.id.clone(),
            Slot { web: web.clone(), near: false, _height: subscription },
        );
        Some(web)
    }

    /// A Visual row. `content_width` is the transcript row's width (the list
    /// viewport less the row insets), when laid out once.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn row(
        &self,
        block: BlockId,
        visual: &Visual,
        content_width: Option<f32>,
        reading: ReadingSize,
        selection: &TextRuns,
        cx: &mut App,
    ) -> AnyElement {
        let grid = Grid::of(reading);
        let narrow = content_width.is_some_and(|w| page_width(w, grid) < MIN_LIVE_WIDTH);
        let stage = match Visuals::engine(cx) {
            _ if !shows_page(visual) => {
                self.inner.borrow_mut().slots.remove(&visual.id);
                if visual.replaced {
                    Stage::Replaced
                } else {
                    Stage::Hidden
                }
            }
            Availability::Unavailable(why) => Stage::Unavailable(why),
            _ if narrow => Stage::Narrow,
            _ => match self.view(visual, reading, cx) {
                Some(web) => Stage::Live(web),
                None => Stage::Waiting,
            },
        };
        card::render(block, visual, stage, self.actions(visual, reading), grid, selection)
    }

    fn actions(&self, visual: &Visual, reading: ReadingSize) -> Actions {
        let html = Rc::new(visual.html.clone());
        let id = visual.id.clone();
        let copied = html.clone();
        Actions {
            open_in_browser: Rc::new(move |_, cx| {
                match write_standalone(&id, &html, &theme_css(reading)) {
                    Ok(path) => cx.open_url(&format!("file://{}", path.display())),
                    Err(error) => eprintln!("ferrite: could not write the visual for a browser: {error}"),
                }
            }),
            copy_html: Rc::new(move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string((*copied).clone()))),
        }
    }

    /// Tell each visual row how close it is to the screen (see the module
    /// docs). `rows` are the transcript's visual rows: (row index, visual).
    /// `height_of` is a row's last measured height, if any. Call after the
    /// list's layout; it acts only on changes.
    pub(crate) fn reach(
        &self,
        list: &ListState,
        rows: &[(usize, &Visual)],
        height_of: &dyn Fn(usize) -> Option<f32>,
        reading: ReadingSize,
        window: &mut Window,
        cx: &mut App,
    ) {
        let viewport = list.viewport_bounds();
        let span = f32::from(viewport.size.height);
        if span <= 0. || rows.is_empty() {
            return;
        }
        let grid = Grid::of(reading);
        let content = f32::from(viewport.size.width) - crate::theme::TX_PAD_L - crate::theme::TX_PAD_R;
        let width = page_width(content, grid);
        let live_width = width >= MIN_LIVE_WIDTH;
        if live_width {
            super::renderer::note_page_width(width);
        }
        for (index, visual) in rows.iter().filter(|(_, visual)| shows_page(visual)) {
            let distance = distance(list, *index, span * FAR, height_of);
            let was = self.inner.borrow().slots.get(&visual.id).is_some_and(|slot| slot.near);
            let near = live_width && if was { distance <= span * FAR } else { distance <= span * NEAR };
            if near == was {
                continue;
            }
            // Near and never drawn: open it now, at the width it will have.
            let web = if near {
                self.view(visual, reading, cx)
            } else {
                self.inner.borrow().slots.get(&visual.id).map(|slot| slot.web.clone())
            };
            let Some(web) = web else { continue };
            if let Some(slot) = self.inner.borrow_mut().slots.get_mut(&visual.id) {
                slot.near = near;
            }
            let reach = if near { Reach::Near { width: px(width) } } else { Reach::Far };
            web.update(cx, |web, cx| web.set_reach(reach, window, cx));
        }
    }

    /// Forget every visual not in `live` (rows that left the transcript):
    /// their pages close.
    pub(crate) fn retain(&self, live: &dyn Fn(&str) -> bool) {
        self.inner.borrow_mut().slots.retain(|id, _| live(id));
    }

    /// How many visuals hold a view (tests).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner.borrow().slots.len()
    }
}

/// Whether a visual's row shows its page at all: not once a later one
/// replaced it, and not when the call failed (the page is whatever the
/// agent got wrong; its words say why, and the fallbacks still open it).
fn shows_page(visual: &Visual) -> bool {
    !visual.replaced && !matches!(visual.status, VisualStatus::Failed(_))
}

/// The page's width in a row `content` px wide: the content column less its
/// frame.
pub(crate) fn page_width(content: f32, grid: Grid) -> f32 {
    (content - grid.gutter() - 2.0).floor().max(0.)
}

/// Px between row `index` and the screen (0 when any of it is on screen),
/// summed no further than `cap`. `ListState` places only measured rows at or
/// below its top row; the rest are summed from `height_of`, else
/// [`ROW_ESTIMATE`].
fn distance(list: &ListState, index: usize, cap: f32, height_of: &dyn Fn(usize) -> Option<f32>) -> f32 {
    let viewport = list.viewport_bounds();
    let top = list.logical_scroll_top();
    let height = |ix: usize| -> f32 {
        list.bounds_for_item(ix)
            .map(|b| f32::from(b.size.height))
            .or_else(|| height_of(ix))
            .unwrap_or(ROW_ESTIMATE)
    };
    if index < top.item_ix {
        // Above: the hidden part of the top row, plus every row between.
        let mut d = f32::from(top.offset_in_item);
        for r in (index + 1..top.item_ix).rev() {
            if d > cap {
                break;
            }
            d += height(r);
        }
        return d;
    }
    if let Some(b) = list.bounds_for_item(index) {
        let gap: Pixels = (b.top() - viewport.bottom()).max(viewport.top() - b.bottom()).max(px(0.));
        return f32::from(gap);
    }
    // Below and not laid out: walk down from the top row.
    let mut y = f32::from(viewport.top() - top.offset_in_item);
    let end = f32::from(viewport.bottom());
    for r in top.item_ix..index {
        y += height(r);
        if y - end > cap {
            break;
        }
    }
    (y - end).max(0.)
}

/// The visual as a page a desktop browser can open: the same theme and the
/// same Content-Security-Policy as inline (nothing fetched), written to a
/// temp file. Returns its path.
fn write_standalone(id: &str, html: &str, theme_css: &str) -> std::io::Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join("ferrite-visuals");
    std::fs::create_dir_all(&dir)?;
    let name: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let path = dir.join(format!("{name}.html"));
    std::fs::write(&path, standalone(html, theme_css))?;
    Ok(path)
}

/// See [`write_standalone`].
fn standalone(html: &str, theme_css: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src 'unsafe-inline' 'unsafe-eval'; style-src 'unsafe-inline'; img-src data: blob:; \
         font-src data:; media-src data: blob:; connect-src 'none'; frame-src 'none'; \
         form-action 'none'; base-uri 'none'\">\
         <style>{theme_css}body{{margin:0;background:var(--ferrite-bg);}}</style>\n{html}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standalone_page_carries_the_theme_and_fetches_nothing() {
        let page = standalone("<p>hi</p>", ":root{--ferrite-bg:#1b1b1c}");
        assert!(page.contains("--ferrite-bg:#1b1b1c"));
        assert!(page.contains("connect-src 'none'"));
        assert!(page.ends_with("<p>hi</p>"));
    }

    #[test]
    fn the_page_is_the_content_column_less_its_frame() {
        let grid = Grid::of(ReadingSize::STANDARD);
        assert_eq!(page_width(600., grid), (600. - grid.gutter() - 2.).floor());
        assert_eq!(page_width(5., grid), 0.);
    }
}
