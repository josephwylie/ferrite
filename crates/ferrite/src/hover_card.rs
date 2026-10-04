//! The hover preview card (the prototype's `#hover`): hovering a path the
//! transcript names shows the file around the line it points at, hung under
//! the path, flipping above it near the window's foot (`hang`).
//!
//! `HOVER_CARD_CELLS` chrome cells wide on the float ground with its 1px
//! `paint::LINE2` edge and the float shadow; it takes no pointer. Its head
//! is the path in `PATH_INK` and `:211` in `TEXT_MUTED`, with `⌘-click opens
//! in a reader pane` at its right; its body six numbered lines,
//! `line-3..line+2`, the target line on `paint::SELECTION` with its number in
//! `ACCENT`, the code syntax-coloured and cut with `…`; its foot, over a
//! `paint::LINE` rule, `modified in this thread · +9−4 · 2 min ago` when
//! the Thread changed the file.
//!
//! State is a window-wide global: a path target on enter names itself and
//! its laid-out box; the transcript hosting it (by namespace) draws the
//! card. `preview_path` asks for a card before the target has laid out; the
//! first target matching it then opens it.

use std::path::Path;
use std::time::{Duration, SystemTime};

use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, Bounds, Context, HighlightStyle, Pixels,
    SharedString, StyledText, Window,
};

use crate::file_links::PathTarget;
use crate::theme;

/// The path the pointer is on, and where.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HoverTarget {
    pub target: PathTarget,
    pub anchor: Bounds<Pixels>,
}

/// The card on screen, if any, and a scene's request for one.
#[derive(Default)]
pub(crate) struct HoverCards {
    current: Option<HoverTarget>,
    /// `preview_path`'s ask: the target (its scope, path and line) to open
    /// once a target naming the same file lays out.
    pending: Option<PathTarget>,
}

impl gpui::Global for HoverCards {}

/// The pointer entered a path target laid out at `anchor`.
pub(crate) fn enter(
    target: &PathTarget,
    anchor: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    // A directory (the banner's checkout) has no lines to preview: it
    // underlines and ⌘-click opens it, but no card hangs.
    if target.scope.is_none() || target.path.is_dir() {
        return;
    }
    let next = HoverTarget {
        target: target.clone(),
        anchor,
    };
    let cards = cx.default_global::<HoverCards>();
    if cards.current.as_ref() != Some(&next) {
        cards.current = Some(next);
        window.refresh();
    }
}

/// The pointer left a path target.
pub(crate) fn leave(target: &PathTarget, window: &mut Window, cx: &mut App) {
    let cards = cx.default_global::<HoverCards>();
    if cards
        .current
        .as_ref()
        .is_some_and(|current| &current.target == target)
    {
        cards.current = None;
        window.refresh();
    }
}

/// Ask for the card on the first target naming `target`'s file in its scope
/// (a scene's `preview_path`): at once when one has laid out, else when one
/// does.
#[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
pub(crate) fn request(target: PathTarget, window: &mut Window, cx: &mut App) {
    let Some(scope) = target.scope.clone() else {
        return;
    };
    match crate::file_links::laid_out(&scope, &target.path, cx) {
        Some(anchor) => {
            cx.default_global::<HoverCards>().current = Some(HoverTarget { target, anchor });
        }
        None => cx.default_global::<HoverCards>().pending = Some(target),
    }
    window.refresh();
}

/// A target laid out: if a scene asked for its card, open it now, anchored
/// here and named as this target names the file.
pub(crate) fn laid_out(target: &PathTarget, anchor: Bounds<Pixels>, cx: &mut App) {
    let Some(pending) = cx
        .try_global::<HoverCards>()
        .and_then(|cards| cards.pending.clone())
    else {
        return;
    };
    let matches =
        target.scope == pending.scope && crate::file_links::same_file(&target.path, &pending.path);
    if matches {
        let cards = cx.default_global::<HoverCards>();
        cards.pending = None;
        cards.current = Some(HoverTarget {
            target: PathTarget {
                line: pending.line.or(target.line),
                ..target.clone()
            },
            anchor,
        });
    }
}

/// The card the transcript hosting `scope` draws now.
pub(crate) fn current(scope: &str, cx: &App) -> Option<HoverTarget> {
    cx.try_global::<HoverCards>()?
        .current
        .clone()
        .filter(|current| current.target.scope.as_deref() == Some(scope))
}

/// Whether `scope` hosts the card now or a scene waits on one there.
pub(crate) fn involves(scope: &str, cx: &App) -> bool {
    cx.try_global::<HoverCards>().is_some_and(|cards| {
        cards
            .current
            .as_ref()
            .is_some_and(|current| current.target.scope.as_deref() == Some(scope))
            || cards
                .pending
                .as_ref()
                .is_some_and(|pending| pending.scope.as_deref() == Some(scope))
    })
}

/// What the card shows about the file beside its lines: the Thread's edits
/// to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CardStat {
    pub added: usize,
    pub removed: usize,
    pub at: Option<SystemTime>,
}

/// The hover card's view: its target, the file's lines around it, and the
/// Thread's edits to it.
pub(crate) struct HoverCard {
    target: PathTarget,
    line: u32,
    lines: Vec<(u32, String)>,
    stat: Option<CardStat>,
}

impl HoverCard {
    /// The card for `target`. Its line: the target's own, else `fallback`
    /// (the first changed line of the Thread's latest edit to the file),
    /// else 1. The window: `line-3 ..= line+2`.
    pub(crate) fn new(target: PathTarget, fallback: Option<u32>, stat: Option<CardStat>) -> Self {
        let line = target.line.or(fallback).unwrap_or(1).max(1);
        let lines = read_window(&target.path, line);
        // The age is the file's own: when it last changed on disk, which
        // the Thread's latest edit made (`2 min ago`), else when that edit
        // settled.
        let modified = std::fs::metadata(&target.path)
            .and_then(|meta| meta.modified())
            .ok();
        let stat = stat.map(|stat| CardStat {
            at: modified.or(stat.at),
            ..stat
        });
        Self {
            target,
            line,
            lines,
            stat,
        }
    }

    pub(crate) fn shows(&self, target: &PathTarget) -> bool {
        &self.target == target
    }
}

/// The six lines around `line` (`line-3 ..= line+2`, clamped to the file's
/// first line), numbered from 1.
fn read_window(path: &Path, line: u32) -> Vec<(u32, String)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let first = line.saturating_sub(3).max(1);
    text.lines()
        .enumerate()
        .skip(first as usize - 1)
        .take(theme::HOVER_CARD_LINES as usize)
        .map(|(index, text)| (index as u32 + 1, text.replace('\t', "    ")))
        .collect()
}

/// An age as the card's foot says it: `just now`, `2 min ago`, `3 h ago`.
pub(crate) fn age_label(at: SystemTime, now: SystemTime) -> String {
    let elapsed = now.duration_since(at).unwrap_or(Duration::ZERO);
    let minutes = elapsed.as_secs() / 60;
    match minutes {
        0 => "just now".to_string(),
        1..=59 => format!("{minutes} min ago"),
        _ => format!("{} h ago", minutes / 60),
    }
}

/// The foot's words and inks: `modified in this thread · +9−4 · 2 min ago`.
pub(crate) fn foot_text(
    stat: &CardStat,
    now: SystemTime,
) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let ink = |color: u32| HighlightStyle {
        color: Some(rgb(color).into()),
        ..Default::default()
    };
    let mut text = String::from("modified in this thread \u{b7} ");
    let added = format!("+{}", stat.added);
    let removed = format!("\u{2212}{}", stat.removed);
    let mut highlights = Vec::new();
    let start = text.len();
    text.push_str(&added);
    highlights.push((start..text.len(), ink(theme::RUNNING)));
    // `+9−4`, the two runs touching (the prototype's flex foot drops the
    // space between them).
    let start = text.len();
    text.push_str(&removed);
    highlights.push((start..text.len(), ink(theme::BLOCKED)));
    if let Some(at) = stat.at {
        text.push_str(" \u{b7} ");
        text.push_str(&age_label(at, now));
    }
    (text, highlights)
}

impl gpui::Render for HoverCard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let cell = theme::CH;
        let language = ferrite_core::transcript::language_for_path(&self.target.path);
        let head_location = format!(":{}", self.line);
        let head = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(theme::LH_UI))
            .px(px(cell))
            .whitespace_nowrap()
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme::PATH_INK))
                            .child(self.target.display()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child(SharedString::from(head_location)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .pl(px(2.0 * cell))
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child(crate::components::key_combo("cmd", theme::TEXT_MUTED))
                    .child("-click opens in a reader pane"),
            );
        let rows = self.lines.iter().map(|(number, code)| {
            let on = *number == self.line;
            let highlights = language
                .map(|language| {
                    let tokens = ferrite_core::transcript::highlight_tokens(Some(language), code);
                    crate::pane::code(code, Some(&tokens))
                })
                .unwrap_or_default();
            div()
                .flex()
                .items_center()
                .h(px(theme::LH_UI))
                .px(px(cell))
                .when(on, |row| row.bg(theme::paint::SELECTION))
                .child(crate::components::tabular(
                    div()
                        .flex_shrink_0()
                        .w(px(theme::HOVER_CARD_NUMBER_CELLS * cell))
                        .pr(px(theme::HOVER_CARD_NUMBER_PAD_CELLS * cell))
                        .text_right()
                        .whitespace_nowrap()
                        .text_color(rgb(if on { theme::ACCENT } else { theme::TEXT_MUTED }))
                        .child(SharedString::from(number.to_string())),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme::SYN_PLAIN))
                        .child(
                            StyledText::new(SharedString::from(code.clone()))
                                .with_highlights(highlights),
                        ),
                )
        });
        let foot = self.stat.map(|stat| {
            let (text, highlights) = foot_text(&stat, ferrite_core::clock::system_time());
            div()
                .flex()
                .items_center()
                .h(px(theme::LH_UI + 1.))
                .px(px(cell))
                .border_t_1()
                .border_color(theme::paint::LINE)
                .whitespace_nowrap()
                .text_color(rgb(theme::TEXT_MUTED))
                .child(StyledText::new(SharedString::from(text)).with_highlights(highlights))
        });
        div()
            .debug_selector(|| "hover-card".into())
            .w(px(theme::HOVER_CARD_CELLS * cell))
            .flex()
            .flex_col()
            .bg(theme::paint::FLOAT)
            .border_1()
            .border_color(theme::paint::LINE2)
            .shadow(crate::components::float_shadow())
            .font_family(theme::FONT_CODE)
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .text_color(rgb(theme::TEXT))
            .child(head)
            .children(rows)
            .children(foot)
    }
}

/// The card's height: its head, its six lines, and its foot (with the
/// rule over it) when the Thread changed the file; its 1px edge each side.
fn card_height(lines: usize, foot: bool) -> f32 {
    let foot = if foot { theme::LH_UI + 1. } else { 0. };
    theme::LH_UI * (1 + lines) as f32 + foot + 2.
}

/// Where the card hangs from its path's box `anchor` in a window
/// `viewport` wide and tall: `HOVER_CARD_GAP` under the path, flipping
/// `HOVER_CARD_GAP` above it when it would cross the window's bottom
/// `HOVER_CARD_EDGE`; its left edge on the path's, held inside the window's
/// edges.
pub(crate) fn hang_origin(
    anchor: Bounds<Pixels>,
    size: gpui::Size<Pixels>,
    viewport: gpui::Size<Pixels>,
) -> gpui::Point<Pixels> {
    let gap = px(theme::HOVER_CARD_GAP);
    let edge = px(theme::HOVER_CARD_EDGE);
    let below = anchor.bottom() + gap;
    let top = if below + size.height <= viewport.height - edge {
        below
    } else {
        (anchor.top() - gap - size.height).max(edge)
    };
    let left = anchor
        .left()
        .min(viewport.width - edge - size.width)
        .max(edge);
    gpui::point(left, top)
}

/// Hang `card` under its path, flipping above when it would overflow. It is
/// painted over everything and takes no pointer.
pub(crate) fn hang(
    card: gpui::Entity<HoverCard>,
    anchor: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (lines, foot) = {
        let card = card.read(cx);
        (card.lines.len(), card.stat.is_some())
    };
    let size = gpui::size(
        px(theme::HOVER_CARD_CELLS * theme::CH),
        px(card_height(lines, foot)),
    );
    let at = hang_origin(anchor, size, window.viewport_size());
    gpui::deferred(gpui::anchored().position(at).child(card))
        .with_priority(3)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_foot_reads_the_threads_edits_and_their_age() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let stat = CardStat {
            added: 9,
            removed: 4,
            at: Some(now - Duration::from_secs(150)),
        };
        let (text, highlights) = foot_text(&stat, now);
        assert_eq!(
            text,
            "modified in this thread \u{b7} +9\u{2212}4 \u{b7} 2 min ago"
        );
        assert_eq!(&text[highlights[0].0.clone()], "+9");
        assert_eq!(&text[highlights[1].0.clone()], "\u{2212}4");
        assert_eq!(age_label(now, now), "just now");
        assert_eq!(
            age_label(now - Duration::from_secs(3 * 3600), now),
            "3 h ago"
        );
        let unknown = CardStat { at: None, ..stat };
        assert_eq!(
            foot_text(&unknown, now).0,
            "modified in this thread \u{b7} +9\u{2212}4"
        );
    }

    #[test]
    fn the_window_is_three_above_and_two_below_the_line() {
        let path =
            std::env::temp_dir().join(format!("ferrite-hover-card-{}.rs", std::process::id()));
        let source: String = (1..=300).map(|n| format!("line {n}\n")).collect();
        std::fs::write(&path, source).unwrap();
        let numbers: Vec<u32> = read_window(&path, 211)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(numbers, [208, 209, 210, 211, 212, 213]);
        let numbers: Vec<u32> = read_window(&path, 1).into_iter().map(|(n, _)| n).collect();
        assert_eq!(numbers, [1, 2, 3, 4, 5, 6]);
        let card = HoverCard::new(PathTarget::new(path.clone()), Some(40), None);
        assert_eq!(
            card.line, 40,
            "the latest edit's line, when the link names none"
        );
        let card = HoverCard::new(
            PathTarget::new(path.clone()).at_line(Some(7)),
            Some(40),
            None,
        );
        assert_eq!(card.line, 7, "the link's own line wins");
        let card = HoverCard::new(PathTarget::new(path), None, None);
        assert_eq!(card.line, 1);
    }
}
