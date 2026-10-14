//! Find (⌘F): a query over the text a surface shows — the focused Pane's
//! transcript, or the reader beside it. Every occurrence is washed
//! (`FIND_WASH`), the one the find is on more strongly (`FIND_ACTIVE_WASH`)
//! and brought into view; ⏎ / ⇧⏎ (⌘G / ⌘⇧G) step through them, wrapping,
//! and esc closes the bar. Smart case: a query with a capital letter
//! matches case, one without ignores it.
//!
//! The bar is a one-row float hung at the top right of the text (the
//! palette's input row: `❯`, the query, then the count). Its host owns
//! what is searched: it hands the bar its text as units, in reading order,
//! and maps the hit the bar is on back to where that text is drawn.

use std::ops::Range;
use std::rc::Rc;

use gpui::base::text::{FindReport, TextFind};
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, App, Context, Div, Entity, HighlightStyle, SharedString, Window};

use crate::composer::{Composer, Edited, Role};
use crate::theme::*;

// ⌘F opens the bar (or takes the keyboard back to it); the others are its
// own keys, in the `Find` context.
gpui::actions!(find, [Open, Next, Previous, Close]);

/// The bar's key context: its line's, where ⏎, ⇧⏎, ⌘G, ⌘⇧G and esc step
/// and close instead of sending, opening a Group or interrupting.
pub(crate) const KEY_CONTEXT: &str = "Find";

/// Frames a host waits for the active occurrence to paint (and report
/// where) before it gives up bringing it into view: a hit in text that is
/// not drawn.
pub(crate) const REVEAL_FRAMES: u8 = 4;

/// What the line asks for: its first line (the text a find counts in puts
/// every run on a line of its own, so a query must not cross one). Smart
/// case: a capital letter anywhere makes the whole query match case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Query {
    pub needle: SharedString,
    pub case_sensitive: bool,
}

impl Query {
    pub(crate) fn new(text: &str) -> Self {
        let text = text.lines().next().unwrap_or_default();
        Self {
            needle: text.to_string().into(),
            case_sensitive: text.chars().any(char::is_uppercase),
        }
    }

    /// Every occurrence in `text`, as byte ranges of it.
    pub(crate) fn matches(&self, text: &str) -> Vec<Range<usize>> {
        gpui::base::text::find_matches(text, &self.needle, self.case_sensitive)
    }
}

/// The marks a native text paints for `query`: every occurrence, and its
/// `active`th (counted in the text) more strongly, reporting where it
/// painted into `report`. None before a query.
pub(crate) fn marks(
    query: &Query,
    active: Option<usize>,
    report: &Rc<FindReport>,
) -> Option<TextFind> {
    if query.needle.is_empty() {
        return None;
    }
    Some(TextFind {
        needle: query.needle.clone(),
        case_sensitive: query.case_sensitive,
        active,
        wash: rgba(FIND_WASH).into(),
        active_wash: rgba(FIND_ACTIVE_WASH).into(),
        report: report.clone(),
    })
}

/// `runs` (ordered, non-overlapping highlights of one line) with each of
/// `washes` laid under its range as a background: a run a wash crosses is
/// split at its edges. Unhighlighted text a wash covers gets the wash alone.
pub(crate) fn washed(
    runs: Vec<(Range<usize>, HighlightStyle)>,
    washes: &[(Range<usize>, u32)],
) -> Vec<(Range<usize>, HighlightStyle)> {
    if washes.is_empty() {
        return runs;
    }
    let mut edges: Vec<usize> = runs
        .iter()
        .map(|(range, _)| range)
        .chain(washes.iter().map(|(range, _)| range))
        .flat_map(|range| [range.start, range.end])
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges
        .windows(2)
        .filter_map(|edge| {
            let span = edge[0]..edge[1];
            let inside = |range: &Range<usize>| range.start <= span.start && span.end <= range.end;
            let run = runs.iter().find(|(range, _)| inside(range));
            let wash = washes.iter().find(|(range, _)| inside(range));
            if run.is_none() && wash.is_none() {
                return None;
            }
            let mut style = run.map(|(_, style)| *style).unwrap_or_default();
            if let Some((_, ink)) = wash {
                style.background_color = Some(rgba(*ink).into());
            }
            Some((span, style))
        })
        .collect()
}

/// One occurrence: the host's unit it lies in, and its ordinal among that
/// unit's occurrences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hit {
    pub unit: usize,
    pub ordinal: usize,
}

/// The bar: its line, the query last searched, what it found, the hit it
/// is on, and what the text it marks reports back.
pub(crate) struct Find {
    field: Entity<Composer>,
    query: Query,
    hits: Vec<Hit>,
    active: Option<usize>,
    report: Rc<FindReport>,
    _edited: gpui::Subscription,
}

impl Find {
    /// A bar whose line calls `edited` on its host at every keystroke.
    pub(crate) fn new<T: 'static>(
        cx: &mut Context<T>,
        edited: impl Fn(&mut T, &mut Context<T>) + 'static,
    ) -> Self {
        let field = cx.new(|cx| {
            let mut line = Composer::new(cx);
            line.set_role(Role::Field(Some(KEY_CONTEXT)), cx);
            line.set_visible_row_limit(1, cx);
            line
        });
        let subscription = cx.subscribe(&field, move |host, _, _: &Edited, cx| edited(host, cx));
        Self {
            field,
            query: Query::default(),
            hits: Vec::new(),
            active: None,
            report: Rc::default(),
            _edited: subscription,
        }
    }

    /// Take the keyboard, the query selected so typing replaces it.
    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        let focus = self.field.read(cx).focus_target(window, cx);
        window.focus(&focus, cx);
        self.field.update(cx, |line, cx| {
            line.select_all(&crate::composer::SelectAll, window, cx)
        });
    }

    /// Whether the line holds the keyboard.
    pub(crate) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        self.field
            .read(cx)
            .focus_target(window, cx)
            .contains_focused(window, cx)
    }

    pub(crate) fn query(&self) -> &Query {
        &self.query
    }

    pub(crate) fn active(&self) -> Option<&Hit> {
        self.hits.get(self.active?)
    }

    pub(crate) fn report(&self) -> &Rc<FindReport> {
        &self.report
    }

    /// Put the bar on hit `at`, when there is one.
    pub(crate) fn go_to(&mut self, at: usize) {
        if at < self.hits.len() {
            self.active = Some(at);
        }
    }

    /// Search `units` (in reading order) for what the line holds. A new
    /// query starts on its first hit in unit `from` or after (the first in
    /// view), wrapping to the first; the same query keeps its index as the
    /// text changes under it (a host that can say which hit that was puts
    /// the bar back on it, `keep`). Whether the query changed.
    pub(crate) fn search<'a>(
        &mut self,
        units: impl IntoIterator<Item = &'a str>,
        from: usize,
        cx: &App,
    ) -> bool {
        let query = Query::new(self.field.read(cx).text());
        let changed = query != self.query;
        self.query = query;
        self.hits = units
            .into_iter()
            .enumerate()
            .flat_map(|(unit, text)| {
                (0..self.query.matches(text).len()).map(move |ordinal| Hit { unit, ordinal })
            })
            .collect();
        self.active = if self.hits.is_empty() {
            None
        } else if changed || self.active.is_none() {
            Some(
                self.hits
                    .iter()
                    .position(|hit| hit.unit >= from)
                    .unwrap_or(0),
            )
        } else {
            self.active.map(|at| at.min(self.hits.len() - 1))
        };
        changed
    }

    /// Stay on `hit` when it is still found: the text moved under the
    /// same query.
    pub(crate) fn keep(&mut self, hit: Hit) {
        if let Some(at) = self.hits.iter().position(|found| *found == hit) {
            self.active = Some(at);
        }
    }

    /// Step to the next hit (or the previous), wrapping.
    pub(crate) fn step(&mut self, forward: bool) -> Option<&Hit> {
        let count = self.hits.len();
        let at = self.active?;
        self.active = Some(if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        });
        self.active()
    }

    /// The marks the native text holding `unit`'s text paints.
    pub(crate) fn marks(&self, unit: usize) -> Option<TextFind> {
        let active = self.active().filter(|hit| hit.unit == unit);
        marks(&self.query, active.map(|hit| hit.ordinal), &self.report)
    }

    /// What the count reads: `3/17`, `no matches`, or nothing before a
    /// query.
    pub(crate) fn count(&self) -> Option<SharedString> {
        if self.query.needle.is_empty() {
            return None;
        }
        Some(match self.active {
            Some(at) => format!("{}/{}", at + 1, self.hits.len()).into(),
            None => "no matches".into(),
        })
    }

    /// The bar: the palette's input row as a float of its own — `❯` in
    /// the accent, the query in `TEXT_STRONG`, the count muted at the
    /// right — hung `FLOAT_OFFSET` under the top right of its host, the
    /// text it searches (`relative`). A host narrower than the bar narrows
    /// the query, never the count.
    pub(crate) fn bar(&self) -> Div {
        let bar = crate::menu::float()
            .debug_selector(|| "find-bar".into())
            .w(px(FIND_W))
            .min_w_0()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(FLOAT_ROW_H))
                    .px(px(FLOAT_PAD_X))
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .w(px(FLOAT_GUTTER))
                            .child(crate::components::prompt_mark(ACCENT)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(FONT_CODE)
                            .line_height(px(LH_UI))
                            .text_color(rgb(TEXT_STRONG))
                            .child(self.field.clone()),
                    )
                    .children(self.count().map(|count| {
                        div()
                            .debug_selector(|| "find-count".into())
                            .flex_shrink_0()
                            .ml(px(FLOAT_DETAIL_GAP))
                            .whitespace_nowrap()
                            .text_color(rgb(TEXT_MUTED))
                            .child(count)
                    })),
            );
        div()
            .absolute()
            .top(px(FLOAT_OFFSET))
            .left(px(TX_PAD_L))
            .right(px(TX_PAD_R))
            .flex()
            .justify_end()
            .child(bar)
    }

    /// Put `text` on the line (what a test types).
    #[cfg(test)]
    pub(crate) fn type_query(&self, text: &str, cx: &mut App) {
        self.field
            .update(cx, |line, cx| line.set(text.to_string(), cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capital_makes_the_query_match_case() {
        assert!(!Query::new("needle").case_sensitive);
        assert!(Query::new("Needle").case_sensitive);
        assert_eq!(Query::new("cargo").matches("Cargo cargo CARGO").len(), 3);
        assert_eq!(Query::new("Cargo").matches("Cargo cargo CARGO"), vec![0..5]);
        // Caseless ranges index the text as written, never overlapping.
        assert_eq!(Query::new("café").matches("café CAFÉ"), vec![0..5, 6..11]);
        assert_eq!(Query::new("aa").matches("aaaa"), vec![0..2, 2..4]);
    }

    #[test]
    fn a_wash_splits_the_runs_it_crosses() {
        let bold = HighlightStyle {
            font_weight: Some(gpui::FontWeight::BOLD),
            ..Default::default()
        };
        let washes = [(2..6, FIND_WASH)];
        let lines = washed(
            vec![(0..4, bold), (4..8, HighlightStyle::default())],
            &washes,
        );
        let spans: Vec<_> = lines.iter().map(|(range, _)| range.clone()).collect();
        assert_eq!(spans, vec![0..2, 2..4, 4..6, 6..8]);
        assert_eq!(lines[0].1.background_color, None);
        assert_eq!(lines[1].1.font_weight, Some(gpui::FontWeight::BOLD));
        assert!(lines[1].1.background_color.is_some());
        assert!(lines[2].1.background_color.is_some());
        assert_eq!(lines[3].1.background_color, None);
        // Text no run styles still takes the wash.
        let bare = washed(Vec::new(), &washes);
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].0, 2..6);
    }

    #[gpui::test]
    fn the_bar_starts_in_view_steps_wrapping_and_keeps_its_place(cx: &mut gpui::TestAppContext) {
        struct Host(Option<Find>);
        let host = cx.new(|cx| {
            let find = Find::new(cx, |_: &mut Host, _| {});
            Host(Some(find))
        });
        let units = ["alpha beta", "gamma", "beta beta", "delta beta"];
        host.update(cx, |host, cx| {
            let find = host.0.as_mut().unwrap();
            find.type_query("beta", cx);
            assert!(find.search(units, 2, cx), "a new query");
            assert_eq!(
                find.active(),
                Some(&Hit {
                    unit: 2,
                    ordinal: 0
                }),
                "the first hit in view"
            );
            assert_eq!(find.count().as_deref(), Some("2/4"));
            assert_eq!(find.step(true).map(|hit| hit.ordinal), Some(1));
            assert_eq!(find.step(true).map(|hit| hit.unit), Some(3));
            assert_eq!(find.step(true).map(|hit| hit.unit), Some(0), "wraps");
            assert_eq!(find.step(false).map(|hit| hit.unit), Some(3), "and back");
            assert!(!find.search(units, 0, cx), "the same query");
            assert_eq!(find.count().as_deref(), Some("4/4"), "keeps its place");
            // The first unit trimmed away: its index now names another
            // occurrence until the host puts the bar back on its own.
            find.go_to(1);
            find.search(units[1..].iter().copied(), 0, cx);
            assert_eq!(find.count().as_deref(), Some("2/3"));
            find.keep(Hit {
                unit: 1,
                ordinal: 0,
            });
            assert_eq!(find.count().as_deref(), Some("1/3"), "the same occurrence");
            find.type_query("BETA", cx);
            find.search(units, 0, cx);
            assert_eq!(find.count().as_deref(), Some("no matches"));
            assert!(find.marks(0).is_some(), "still washes");
            find.type_query("", cx);
            find.search(units, 0, cx);
            assert_eq!(find.count(), None);
            assert!(find.marks(0).is_none());
        });
    }
}
