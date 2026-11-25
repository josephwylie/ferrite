//! A changed file's hunks as a diff draws them: the Review core's rows cut
//! to each change with one line of context either side, and the area that
//! draws them in the panel's diff face (a sign column, the add and delete
//! washes, no line numbers and no syntax colour). The Overview's file blocks
//! draw them abbreviated, the Changes Surface whole.

use ferrite_reviewr::diff::Row;
use ferrite_reviewr::ferrite::FileSides;
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, Div, SharedString};

/// Context lines kept beside each change. Hunks closer than this merge.
const CONTEXT: usize = 1;

/// The most lines an abbreviated file's hunks draw; past them, `… +N lines`
/// (the whole diff is the Changes Surface's).
const MAX_LINES: usize = 120;

/// How a file's hunks are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// The Overview's block: the first `MAX_LINES` lines, each cut at the edge.
    Abbreviated,
    /// The Changes Surface: every line. A long line wraps, or, unwrapped,
    /// scrolls sideways.
    Whole { wrap: bool },
}

/// What a file's block shows under its head once its read lands.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Shown {
    /// Its hunks, in file order, each a run of lines.
    Hunks(Vec<Vec<Line>>),
    /// Why there are none: not in the uncommitted diff, binary, too large,
    /// or the read failed.
    Note(SharedString),
}

/// One drawn line of a hunk.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Line {
    pub sign: Sign,
    pub text: SharedString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sign {
    Context,
    Added,
    Removed,
}

/// A file's sides as the panel shows them: its hunks, or why there are none.
pub fn shown(sides: Option<&FileSides>) -> Shown {
    let Some(sides) = sides else {
        return Shown::Note("No uncommitted changes".into());
    };
    let diff = sides.rows();
    if let Some(notice) = diff.notice {
        use ferrite_reviewr::diff::Notice;
        return Shown::Note(
            match notice {
                Notice::Binary => "Binary file",
                Notice::TooLarge => "File too large to show",
                Notice::Unreadable => "Couldn't read this file",
            }
            .into(),
        );
    }
    let hunks = hunks(&diff.rows);
    if hunks.is_empty() {
        return Shown::Note("No uncommitted changes".into());
    }
    Shown::Hunks(hunks)
}

/// The Review core's diff rows cut to the panel's hunks: each changed line
/// with `CONTEXT` lines beside it. A single unchanged line between two
/// hunks is drawn rather than cut, as the Review's own folds do.
fn hunks(rows: &[Row]) -> Vec<Vec<Line>> {
    let mut lines = Vec::new();
    flatten(rows, &mut lines);
    let n = lines.len();
    let mut keep = vec![false; n];
    for (at, line) in lines.iter().enumerate() {
        if line.sign != Sign::Context {
            let hi = (at + CONTEXT).min(n - 1);
            keep[at.saturating_sub(CONTEXT)..=hi].fill(true);
        }
    }
    for at in 1..n.saturating_sub(1) {
        if !keep[at] && keep[at - 1] && keep[at + 1] {
            keep[at] = true;
        }
    }
    let mut hunks: Vec<Vec<Line>> = Vec::new();
    let mut open = false;
    for (line, kept) in lines.into_iter().zip(keep) {
        match (kept, open) {
            (true, true) => hunks.last_mut().expect("an open hunk").push(line),
            (true, false) => hunks.push(vec![line]),
            (false, _) => {}
        }
        open = kept;
    }
    hunks
}

/// Every content row, folds opened, as a line.
fn flatten(rows: &[Row], out: &mut Vec<Line>) {
    let text = |spans: &[ferrite_reviewr::diff::Span]| -> SharedString {
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>()
            .replace('\t', "    ")
            .into()
    };
    for row in rows {
        let (sign, spans) = match row {
            Row::Context { spans, .. } => (Sign::Context, spans),
            Row::Insertion { spans, .. } => (Sign::Added, spans),
            Row::Deletion { spans, .. } => (Sign::Removed, spans),
            Row::Fold { lines } => {
                flatten(lines, out);
                continue;
            }
            Row::Rendered { .. } => continue,
        };
        out.push(Line {
            sign,
            text: text(spans),
        });
    }
}

/// The hunk area (`BLOCK`, the panel's diff face): each line on a 2.5ch sign
/// column inset 1ch, changed lines on their wash in body ink, context in
/// `INK_SUBTLE`; a hairline between hunks.
pub fn hunks_element(index: usize, shown: &Shown, layout: Layout) -> Div {
    use ferrite_ui::theme::*;
    let cell = FS_PANEL_DIFF * CODE_ADVANCE;
    let wrap = layout == Layout::Whole { wrap: true };
    let scrolls = layout == Layout::Whole { wrap: false };
    // A wrapped line grows by rows; an unwrapped one is cut at the edge, or
    // keeps its width for the area to scroll.
    let row = || {
        div()
            .flex()
            .flex_shrink_0()
            .when(wrap, |row| row.min_h(px(LH_PANEL_DIFF)))
            .when(!wrap, |row| row.h(px(LH_PANEL_DIFF)))
            .pl(px(PANEL_HUNK_INSET_CELLS * cell))
            .when(!scrolls, |row| row.overflow_hidden())
    };
    let area = div()
        .debug_selector(move || format!("panel-hunks-{index}"))
        .flex()
        .flex_col()
        .pt(px(PANEL_HUNK_PAD_T))
        .pb(px(PANEL_HUNK_PAD_B))
        .bg(paint::BLOCK)
        .font_family(FONT_CODE)
        .text_size(px(FS_PANEL_DIFF))
        .line_height(px(LH_PANEL_DIFF))
        .when(!wrap, |area| area.whitespace_nowrap());
    let hunks = match shown {
        Shown::Hunks(hunks) => hunks,
        Shown::Note(note) => {
            return area.child(row().text_color(rgb(INK_SUBTLE)).child(note.clone()));
        }
    };
    let total: usize = hunks.iter().map(Vec::len).sum();
    let mut left = if layout == Layout::Abbreviated { MAX_LINES } else { usize::MAX };
    let mut drawn = 0;
    let mut area = area;
    for (at, hunk) in hunks.iter().enumerate() {
        if left == 0 {
            break;
        }
        if at > 0 {
            area = area.child(
                div()
                    .flex_shrink_0()
                    .h(px(1.))
                    .my(px(PANEL_HUNK_PAD_T))
                    .bg(paint::LINE),
            );
        }
        for line in hunk.iter().take(left) {
            let (glyph, sign_ink, ink, wash) = match line.sign {
                Sign::Context => ("", INK_SUBTLE, INK_SUBTLE, None),
                Sign::Added => ("+", DIFF_ADDED_SIGN, SYN_PLAIN, Some(DIFF_ADDED_WASH)),
                Sign::Removed => (
                    "\u{2212}",
                    DIFF_REMOVED_SIGN,
                    SYN_PLAIN,
                    Some(DIFF_REMOVED_WASH),
                ),
            };
            let n = drawn;
            area = area.child(
                row()
                    .debug_selector(move || format!("panel-hunk-{index}-{n}"))
                    .when_some(wash, |row, wash| row.bg(rgba(wash)))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(PANEL_HUNK_SIGN_CELLS * cell))
                            .text_color(rgb(sign_ink))
                            .child(glyph),
                    )
                    .child(
                        div()
                            .when(scrolls, |text| text.flex_none())
                            .when(!scrolls, |text| text.flex_1().min_w_0().overflow_hidden())
                            .text_color(rgb(ink))
                            .child(line.text.clone()),
                    ),
            );
            drawn += 1;
        }
        left = left.saturating_sub(hunk.len());
    }
    if total > drawn {
        area = area.child(
            row()
                .pl(px((PANEL_HUNK_INSET_CELLS + PANEL_HUNK_SIGN_CELLS) * cell))
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(format!(
                    "\u{2026} +{} lines",
                    total - drawn
                ))),
        );
    }
    if scrolls {
        // The area is as wide as its longest line (at least the column);
        // the scroller carries it sideways.
        return div().flex().flex_col().child(
            div()
                .id(SharedString::from(format!("panel-hunks-scroll-{index}")))
                .debug_selector(move || format!("panel-hunks-scroll-{index}"))
                .flex()
                .overflow_x_scroll()
                .child(area.flex_none().min_w_full()),
        );
    }
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(text: &str) -> Row {
        Row::Context {
            old_no: 0,
            new_no: 0,
            spans: vec![span(text)],
        }
    }

    fn added(text: &str) -> Row {
        Row::Insertion {
            new_no: 0,
            spans: vec![span(text)],
            emphasis: vec![],
            cr: false,
        }
    }

    fn removed(text: &str) -> Row {
        Row::Deletion {
            old_no: 0,
            spans: vec![span(text)],
            emphasis: vec![],
            cr: false,
        }
    }

    fn span(text: &str) -> ferrite_reviewr::diff::Span {
        ferrite_reviewr::diff::Span {
            text: text.into(),
            color: Default::default(),
        }
    }

    fn signs(hunks: &[Vec<Line>]) -> Vec<String> {
        hunks
            .iter()
            .map(|hunk| {
                hunk.iter()
                    .map(|line| match line.sign {
                        Sign::Context => ' ',
                        Sign::Added => '+',
                        Sign::Removed => '-',
                    })
                    .collect()
            })
            .collect()
    }

    /// One line of context either side of a change, the Review's folds
    /// opened; a lone unchanged line between two changes is drawn, two
    /// split the hunks.
    #[test]
    fn hunks_keep_one_line_of_context() {
        let rows = vec![
            context("a"),
            Row::Fold {
                lines: vec![context("b"), context("c")],
            },
            context("d"),
            removed("e"),
            added("E"),
            context("f"),
            context("g"),
            added("h"),
            context("i"),
            context("j"),
            context("k"),
            context("l"),
            added("m"),
        ];
        let hunks = hunks(&rows);
        assert_eq!(signs(&hunks), [" -+  + ", " +"]);
        assert_eq!(hunks[0][0].text, "d");
        assert_eq!(hunks[1][0].text, "l");
    }
}
