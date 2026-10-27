//! The board's screen drawn: each line `view::screen` lays out, piece by
//! piece, on Ferrite's one cell grid in the colour its named ink has in the
//! board's theme — what upstream's terminal drawer (`board/run/draw.rs`)
//! does with ratatui cells.
//!
//! Every piece starts at the cell the board measured for it. A glyph
//! Geist Mono lacks (the marks `◐ ✓ ◆`, the box's `╭╮`, CJK, emoji) is set
//! by a fallback face of its own advance, so it gets a box of exactly its
//! cells; runs of the face's own glyphs are drawn as one text each. The
//! words are set in the UI face on the code face's cells, so a run is never
//! clipped to its box: a bold run of wide letters (the menu's `Menu`) may
//! reach a little into the cell after it, which the board leaves blank
//! between pieces, rather than lose its last glyph; a fallback glyph wider
//! than its cells (the marks) is centred on them, whole.
//!
//! The theme is `board.theme`: `herdr` (and `dark`, Ferrite having one
//! appearance) is Ferrite's own colours under the board's rules
//! (`docs/colours.md`: the greys held readable, its own review blue, queue
//! pink, todo grey and automations purple, worked out by the vendored
//! `theme::greys` and `theme::picks`); `light` and a herdr theme's name are
//! that theme from herdr's palettes, on its own ground. With `NO_COLOR` set
//! nothing is painted: bold, reverse and the marks say everything.

use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, SharedString};
use sw_board::board::theme::{self, greys, picks, Pal, Seen, Theme, C};
use sw_board::shown::ink::{Ink, Inked, Piece};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::{
    BLUE, CARD, CH, CYAN, FIELD, FRAME, GREEN, LINE, LINE2, MAGENTA, ORANGE, RED, ROW, SELECTED,
    TEXT, TEXT_DIM, TEXT_STRONG, W_STRONG, YELLOW,
};

/// Ferrite's own theme's name, as `Theme::name` carries it.
pub(crate) const FERRITE: &str = "ferrite";

/// The brighter grey (herdr's `subtext0`): words that have to be read,
/// between `TEXT_DIM` and `TEXT`.
const SOFT: u32 = 0xb6b6bb;

/// The theme `board.theme` is set to. Reads the settings file: off the UI
/// thread.
pub(crate) fn theme_set() -> Theme {
    let want = sw_board::store::settings::lookup::setting("board.theme", None);
    theme_named(want.as_str().unwrap_or("herdr"))
}

/// The theme `board.theme` names: `herdr` (or `auto`, `dark`) is Ferrite's.
pub(crate) fn theme_named(want: &str) -> Theme {
    let plain = theme::no_color();
    if matches!(want, "" | "herdr" | "auto" | "dark") {
        return ferrite_theme(plain);
    }
    let resolved = theme::resolve(&toml::Table::new(), None, want, Seen::default());
    Theme {
        pal: resolved.pal,
        ground: resolved.ground,
        symbols: resolved.symbols,
        plain,
        name: resolved.name,
        web: false,
    }
}

/// Ferrite's colours as herdr's tokens, finished as `theme::resolve`
/// finishes a herdr theme: its greys held readable on the reading plane,
/// and the board's own colours picked beside them.
pub(crate) fn ferrite_theme(plain: bool) -> Theme {
    let own = |rgb: u32| C::Rgb(rgb);
    let ground = own(CARD);
    let text = own(TEXT);
    let mut pal = Pal {
        accent: own(TEXT_STRONG),
        panel_bg: ground,
        sidebar_bg: own(FRAME),
        active_row_bg: own(SELECTED),
        selection_bg: own(SELECTED),
        surface0: own(FIELD),
        surface1: own(LINE2),
        surface_dim: own(LINE),
        overlay0: own(TEXT_DIM),
        overlay1: own(TEXT_DIM),
        text,
        subtext0: own(SOFT),
        mauve: own(MAGENTA),
        green: own(GREEN),
        yellow: own(YELLOW),
        red: own(RED),
        blue: own(BLUE),
        teal: own(CYAN),
        peach: own(ORANGE),
        review: C::Own,
        check: C::Own,
        todo: C::Own,
        auto: C::Own,
        queue: C::Own,
    };
    greys::readable_greys(&mut pal, &|_, _| false, ground, text, true);
    pal.review = picks::review_blue(&pal);
    pal.check = pal.subtext0;
    pal.todo = greys::todo_grey(&pal, ground, text);
    pal.auto = picks::auto_purple(&pal);
    pal.queue = picks::queue_pink(&pal, ground);
    Theme {
        pal,
        ground,
        symbols: true,
        plain,
        name: FERRITE.into(),
        web: false,
    }
}

/// What the board paints under its lines: nothing on Ferrite's own theme
/// (the reading plane shows), a named theme's own ground.
pub(crate) fn ground(theme: &Theme) -> Option<u32> {
    (theme.name != FERRITE && !theme.plain)
        .then(|| theme.ground.drawn())
        .flatten()
}

/// One stretch of a line in one style, `cells` wide.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    pub text: String,
    pub cells: usize,
    pub fg: u32,
    pub bg: Option<u32>,
    pub bold: bool,
    pub under: bool,
}

/// A named ink's colour in `theme`, as the board paints a piece of it:
/// `Plain` is the text's, and with `NO_COLOR` every ink is. The one place
/// board inks become colours, the nav's status inks too.
pub(crate) fn ink(theme: &Theme, ink: Ink) -> u32 {
    let text = theme.ink(Ink::Text).drawn().unwrap_or(TEXT);
    if theme.plain || ink == Ink::Plain {
        return text;
    }
    theme.ink(ink).drawn().unwrap_or(text)
}

/// A piece's colours, as `draw::style_of` works them out: its ink (made
/// quieter, or dim), its background ink, then reverse.
fn colours(theme: &Theme, piece: &Piece) -> (u32, Option<u32>) {
    let text = ink(theme, Ink::Text);
    let ground = theme.ground.drawn().unwrap_or(CARD);
    let (mut fg, mut bg) = (text, None);
    if !theme.plain {
        fg = if piece.quiet {
            theme.quiet(piece.c).drawn().unwrap_or(text)
        } else {
            ink(theme, piece.c)
        };
        bg = piece.bg.and_then(|bg| theme.ink(bg).drawn());
        if piece.dim {
            fg = greys::mix(fg, ground, 0.5);
        }
    }
    if piece.inv {
        return (bg.unwrap_or(ground), Some(fg));
    }
    (fg, bg)
}

/// Whether Geist Mono sets this grapheme itself, one cell wide: ASCII and
/// the face's own marks (`theme::CHROME_GLYPHS`).
fn in_face(grapheme: &str) -> bool {
    const FACE: &str = "●○└├│─·•…↑↓←→↳↩⇧⇥⏎↵⌫±−—×›‹▲▼▶◀";
    let mut chars = grapheme.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_ascii() || FACE.contains(c),
        _ => false,
    }
}

/// A line as runs: each piece split where a glyph needs a box of its own;
/// zero-width graphemes dropped, as the terminal drawer drops them.
pub(crate) fn runs(line: &Inked, theme: &Theme) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for piece in &line.0 {
        let (fg, bg) = colours(theme, piece);
        let run = |text: &str, cells| Run {
            text: text.to_string(),
            cells,
            fg,
            bg,
            bold: piece.bold,
            under: piece.under,
        };
        let mut joined: Option<Run> = None;
        for grapheme in piece.t.graphemes(true) {
            let cells = sw_board::text::grapheme_width(grapheme);
            if cells == 0 {
                continue;
            }
            if in_face(grapheme) {
                let open = joined.get_or_insert_with(|| run("", 0));
                open.text.push_str(grapheme);
                open.cells += cells;
            } else {
                out.extend(joined.take());
                out.push(run(grapheme, cells));
            }
        }
        out.extend(joined);
    }
    out
}

/// The screen's lines as elements, one grid row each.
pub(crate) fn lines(lines: &[Inked], theme: &Theme) -> Vec<AnyElement> {
    lines
        .iter()
        .enumerate()
        .map(|(at, line)| {
            div()
                .id(("board-line", at))
                .flex()
                .flex_none()
                .h(px(ROW))
                .overflow_hidden()
                .whitespace_nowrap()
                .children(runs(line, theme).into_iter().map(run_element))
                .into_any_element()
        })
        .collect()
}

fn run_element(run: Run) -> AnyElement {
    // A glyph of a fallback face (a row's `◐` mark) is often wider than its
    // cell: centred, it shares the overflow with the blank cells either side
    // instead of being cut to its left half.
    let fallback = run.text.graphemes(true).nth(1).is_none() && !in_face(&run.text);
    div()
        .flex_none()
        .w(px(run.cells as f32 * CH))
        .h(px(ROW))
        .when(fallback, |el| el.flex().justify_center())
        .text_color(rgb(run.fg))
        .when_some(run.bg, |el, bg| el.bg(rgb(bg)))
        .when(run.bold, |el| el.font_weight(W_STRONG))
        .when(run.under, |el| el.text_decoration_1())
        .child(SharedString::from(run.text))
        .into_any_element()
}
