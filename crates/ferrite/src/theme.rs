//! Ferrite's visual system: every colour, face, size and metric, named once.
//! This module doc is where the design rules live; there is no other design
//! document. Render code imports from here and holds no colour or metric
//! literal of its own; core stays colour-blind. The approved picture these
//! rules describe is `spikes/terminal-native/index.html` (its `:root`
//! tokens, then its `.clean` and `.glass` blocks: grey, clean, glass).
//!
//! **Terminal-native, glass on the frame.** Ferrite reads like the provider
//! CLIs it drives — one monospace grid, `❯` prompts, `●` tool bullets, `└`
//! results, real terminal colour — and keeps what only a native app can do:
//! vector marks, inline images, side-by-side diffs, folds in place, pixel
//! scrolling, floating pickers. The frame around the terminal (sidebar,
//! titlebar, bottom bar) is macOS glass; the reading plane stays a terminal
//! surface. No chat bubbles, no avatars, no cards, no pills, no centred
//! column of proportional prose.
//!
//! The rules every render site follows:
//!
//! 1. **One face, one grid.** Geist Mono (bundled) is the only face, on
//!    every surface: sidebar, titlebar, menus, sheets, prose, code.
//!    `FONT_UI` and `FONT_CODE` both name it (two names so call sites keep
//!    saying what the text is). A surface sets one size on one line height:
//!    the chrome `FS_UI` 13 on `LH_UI` 20; the transcript the operator's
//!    reading size (`answer_text_size`, cmd-= / cmd-- / cmd-0) on its 1.5×
//!    line (`answer_line_height`), and everything inside a transcript scales
//!    with it. Vertical metrics are whole or half rows (`ROW`, `HALF_ROW`);
//!    horizontal ones are cells (`CH`, the face's 0.6em advance). Hierarchy
//!    comes from ink, weight, colour and the 2-cell glyph gutter, never from
//!    size or face: a heading is the body size (`heading_scale` is 1), and
//!    `FS_SM`, `FS_PROSE_SM`, `LH_META` survive only as legacy names for the
//!    one size. Weights: 400 body; 500 a surface's one title, a tool's name,
//!    a label; 600 the prompt `❯`, strong prose, headings, a Project heading.
//!    700 is unused. Never reach a weight by family name.
//! 2. **Square and flat inside.** Every in-app radius is 0 (the `R_*`
//!    names stay so call sites compile). No card, no drop shadow, no top
//!    light, no pill, no well, no emboss. Separation is a 1px line
//!    (`paint::LINE`, `paint::LINE2`) or a full-width shaded band
//!    (`paint::BAND`, `paint::HEAD`, `paint::INBAND`). Only a floating
//!    surface — menu, picker, palette, popover, tooltip, toast — casts, one
//!    small shadow (`components::float_shadow`); a modal adds `VEIL`.
//!    `components::elevation` keeps its rungs as names and paints nothing
//!    but the float's shadow.
//! 3. **Grey, not near-black.** The opaque ladder, darkest first: `NODIFF`
//!    (a split diff's empty side) < `PLANE` #1b1b1c (the reading plane: a
//!    Pane's body, the empty board) < `HUNK` (a hunk's header row) <
//!    `CHROME` #242425 (the sidebar, the titlebar cell over it, the bottom
//!    bar; also `HEAD`, the focused Pane's head band, and `INBAND`, the
//!    Composer's input band) < `FLOAT` (floating ground) < `HOVER` #2c2c2e
//!    (under the pointer) < `BAND` #2e2e30 (the prompt echo, sticky over its
//!    turn; a diff's file head; a Decision's command well) < `SELECTION`
//!    #343437 (the selected row, the menu cursor) < `BAND2` (the current
//!    bottom-bar tab, a chip) < `SELECTION_HOVER` (a selected row under the
//!    pointer, every press). Lines: `LINE` #2f2f31 for every rule and seam,
//!    `LINE2` #3c3c3f for a float's edge, a table's head rule, a code block's
//!    rail. The 1px seams between Panes on a board are lines, not gutters.
//! 4. **Glass on the frame, terminal in the content** (macOS: `GLASS`). The
//!    window opens blurred (`window_background`) and its root paints nothing
//!    (`paint::WINDOW`); each region paints its own fill once, from
//!    `paint::*`, translucent on macOS: chrome at 72%, the reading plane at
//!    94% (a hint of the desktop, never a busy picture under text, the way
//!    Ghostty and iTerm do background opacity), the head and input bands,
//!    hover and selection as white overlays (5 / 6 / 11%), lines as white at
//!    8 / 14%. gpui blurs no single element, so a float paints near-opaque
//!    (`paint::FLOAT`, 96%) to stay legible on its own, and a band that must
//!    occlude what scrolls under it (the sticky prompt) is painted over its
//!    plane, never over the bare window. Every other platform paints the
//!    opaque greys of rule 3. Fills and lines go through `theme::paint`
//!    (typed `Paint`, so `rgb()` cannot swallow one by mistake); the bare
//!    `PLANE`, `CHROME`, … consts are the opaque values, for contrast math and
//!    the few places that must be opaque. **Never stack two region fills**:
//!    a region paints once over the window; what sits inside it paints an
//!    overlay (`HEAD`, `HOVER`, `SELECTION`, `BAND2`) or nothing.
//! 5. **An ink ladder with floors**, brightest first: `TEXT_STRONG` #f4f4f5
//!    (titles, the prompt text, headings, a tool's name, `**strong**`),
//!    `TEXT` #d7d7d9 (agent prose, body), `TEXT_MUTED` #98989d (metadata,
//!    arguments, results, durations, the sidebar's state words; the floor for
//!    text that must be read: 4.5:1 on every plane a row rests on, 4:1 on a
//!    selected row), `TEXT_FAINT` #5e5e63 (structure only: elbows, rails,
//!    the `·` seam, the parked ring). **`TEXT_FAINT` is never text.** A row
//!    spends at most two text inks, one glyph ink and one colour.
//! 6. **Real terminal colour.** `ACCENT` steel blue #8eb1f0 is the
//!    operator's: the prompt `❯`, the caret, the focused Pane's border,
//!    links, the current row's `❯`, a selected choice, the primary action.
//!    State: `RUNNING` green #93cf8c (live work, `ok`, diff `+`), `ATTENTION`
//!    yellow #e6c47c (needs you), `BLOCKED` red #ef8a80 (failed, diff `−`);
//!    green never means "finished". Syntax: `SYN_KEYWORD` magenta,
//!    `SYN_FUNCTION` blue, `SYN_STRING` green, `SYN_NUMBER`/`SYN_CONST`
//!    orange, `SYN_TYPE` yellow, comments `TEXT_MUTED` italic, punctuation
//!    `TEXT_MUTED`; inline code and file paths `CYAN` (`INLINE_CODE`,
//!    `PATH_INK`); the permission mode word `MODE_INK` (magenta). A diff row
//!    washes red or green (`DIFF_*_WASH`, the changed words `DIFF_*_WORD`)
//!    under its code's own colours; the sign carries the hue. A provider's
//!    brand colour (`PROVIDER_CLAUDE`, `PROVIDER_CODEX`) is only on its
//!    logomark and its working spinner. **Colour sits on the word**: a state
//!    shows once per surface and never tints a ground (the ochre wash stays
//!    retired; a waiting Pane's border is `ATTENTION_EDGE`).
//! 7. **Clean: one place per fact.** A Pane head carries no state word and
//!    no number — the dot (or the braille spinner), the title, the provider
//!    mark; state reads at the foot of the Pane (the working line, the turn's
//!    end, the Decision). Sidebar rows carry no provider marks, and their
//!    state words are `TEXT_MUTED` (the dot carries the colour). The bottom
//!    bar shows no counts. An unfocused Pane keeps its Composer's input row
//!    and hides its status line.
//! 8. **Little moves at once.** What moves: the working line's spinner
//!    (`components::working_spinner`, `· ✢ ✳ ✶ ✻ ✽` at
//!    `MOTION_WORKING_FRAME_MS`, the provider's colour) and its caption's
//!    shimmer (`components::shimmer`), on the focused Pane only; the braille
//!    spinner (`components::braille_spinner`, `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` at
//!    `MOTION_BRAILLE_FRAME_MS`, `RUNNING`) in place of the dot of every
//!    working Thread, in the sidebar and in Pane heads; the soft block caret
//!    blinking in the focused Composer (`components::caret_blink`); the
//!    sidebar collapsing (cmd-B, `MOTION_RESIZE_MS`, its content fading).
//!    **No dot pulses or breathes**; unread is ink (`TEXT_STRONG` title), not
//!    motion. The motion section below holds the rest of the catalog, and
//!    reduced motion holds every loop on a static end state that says the
//!    same thing.
//! 9. **Pixel line heights and a space scale.** Every text role is a (size,
//!    line height) pair, and fixed row heights are `const` expressions of
//!    those pairs, never hand-summed literals. Space: 2 · 4 · 6 · 8 · 12 ·
//!    16 · 24 · 32 (`SPACE_*`, named in gpui's 4px units) and, on the grid,
//!    `ROW` · `HALF_ROW` · `CH`. A metric off them says why in its doc.
//! 10. **Glyph coverage.** A glyph outside Geist Mono's cmap is never text
//!     on any surface; it is an SVG in a glyph box (`icons.rs`,
//!     `components::glyph_box`). `CHROME_GLYPHS` lists the non-ASCII glyphs
//!     text may use (`● ○ └ ├ │ ─ · ↑ ↓ ⇧ ⇥ ⏎ …`), and a test checks each
//!     against the face; `DRAWN_GLYPHS` pins the ones that must stay drawn
//!     (`❯ ✻ ◆ ⏵ ✓ ✗ ▾ ▸ ⌘ ⎿ ∴`, the spinner frames).
//! 11. **Words.** Ferrite's own copy speaks one shared word list
//!     (`theme::words`, beside the state inks and tested against the
//!     notifications, the Decision card and the transcript): `needs you`,
//!     `failing N`, `failed`, `interrupted`, `working`, `done`. Titles,
//!     buttons, menu items, labels and empty states are sentence case
//!     (`New thread`, `Delete thread`); state and value tokens are always
//!     lowercase, even leading a row; Title Case lives only in the macOS menu
//!     bar. `·` (in `TEXT_FAINT`) is the only separator inside a line — no
//!     colon labels, no em dash, no final period on one-line copy — and no
//!     surface prints `now`. A shortcut in a tooltip is a `TEXT_MUTED`
//!     suffix with glyph modifiers (`Toggle sidebar ⌘B`), read from the
//!     keymap, never typed by hand.
//!
//! **Layout of this file.** Everything down to `init_components` is the frozen
//! shared head: values more than one work package reads, the paints, and the
//! kit mapping. Then a block of **legacy aliases** — the previous design's
//! names, mapped onto the new tokens so every module still builds while the
//! surfaces are restyled; it is removed after integration. Below it, one
//! section per work package (`WP-A` … `WP-G`), each opened by a banner and
//! closed by an `(end WP-x)` line. A package edits values and appends tokens
//! only inside its own section; a token two packages need is a request to
//! the integrator, who adds it to the head.
//!
//! The contrast floors, the ladder order, the paints, the kit-token mapping
//! and the derived row heights are asserted in `theme::tests`. Dark only:
//! operators work long sessions beside dark editors and terminals.

use gpui::FontWeight;

// --------------------------------------------------------------- platform

/// Glass on the frame (rule 4): macOS opens the window blurred and paints
/// translucent fills; every other platform paints the opaque greys.
pub const GLASS: bool = cfg!(target_os = "macos");

/// An opaque `0xRRGGBB` as a `0xRRGGBBAA` paint.
pub const fn solid(rgb: u32) -> u32 {
    (rgb << 8) | 0xff
}

/// A fill or a line whose value depends on the platform (rule 4):
/// `0xRRGGBBAA`, translucent glass on macOS and the opaque grey elsewhere.
/// It converts into `Hsla`, `Rgba` and `Fill`, so `.bg(paint::PLANE)` and
/// `.border_color(paint::LINE)` just work, and `rgb()` refuses it: the one
/// mistake a bare `u32` would let through silently (`rgb(0x1a1a1bf0)` drops
/// the red byte and paints blue).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Paint(pub u32);

impl Paint {
    /// `glass` on macOS, `opaque` elsewhere, both `0xRRGGBBAA`.
    pub const fn pick(glass: u32, opaque: u32) -> Paint {
        if GLASS {
            Paint(glass)
        } else {
            Paint(opaque)
        }
    }

    /// The raw `0xRRGGBBAA`, for an API that takes one (`rgba()`).
    pub const fn rgba(self) -> u32 {
        self.0
    }

    /// Whether nothing under it shows through.
    pub const fn is_opaque(self) -> bool {
        self.0 & 0xff == 0xff
    }

    pub fn hsla(self) -> gpui::Hsla {
        gpui::rgba(self.0).into()
    }
}

impl From<Paint> for gpui::Rgba {
    fn from(paint: Paint) -> Self {
        gpui::rgba(paint.0)
    }
}

impl From<Paint> for gpui::Hsla {
    fn from(paint: Paint) -> Self {
        gpui::rgba(paint.0).into()
    }
}

impl From<Paint> for gpui::Fill {
    fn from(paint: Paint) -> Self {
        gpui::rgba(paint.0).into()
    }
}

// ---------------------------------------------------------------- planes
//
// Each plane's opaque value (rule 3, `rgb()`), then its glass value
// (`0xRRGGBBAA`). Paint a region with `paint::*`, which picks between them.

/// `#1b1b1c` — the reading plane: a Pane's body, the empty board, the
/// titlebar cell over the board. The darkest working plane.
pub const PLANE: u32 = 0x1b1b1c;
/// The reading plane as glass: 94%, a terminal's background opacity.
#[allow(dead_code)]
pub const PLANE_GLASS: u32 = 0x1a1a1bf0;
/// `#242425` — chrome, lifted off the plane: the sidebar, the titlebar cell
/// over it, the bottom bar.
pub const CHROME: u32 = 0x242425;
/// Chrome as glass: thin, 72%.
#[allow(dead_code)]
pub const CHROME_GLASS: u32 = 0x242427b8;
/// `#242425` — the focused Pane's head band: chrome's value, on the plane.
#[allow(dead_code)]
pub const HEAD: u32 = 0x242425;
/// The head band as glass: a white overlay on the plane.
#[allow(dead_code)]
pub const HEAD_GLASS: u32 = 0xffffff0d;
/// `#242425` — the Composer's input band, full width at the Pane's foot.
#[allow(dead_code)]
pub const INBAND: u32 = 0x242425;
#[allow(dead_code)]
pub const INBAND_GLASS: u32 = 0xffffff0d;
/// `#2e2e30` — a band: the prompt echo (sticky over its turn), a diff's
/// file head, a Decision's command well.
pub const BAND: u32 = 0x2e2e30;
/// A band as glass: 80%, painted over the plane, where it composes to ~99%
/// — the sticky prompt band must occlude what scrolls under it.
#[allow(dead_code)]
pub const BAND_GLASS: u32 = 0x343437cc;
/// `#363638` — a band a step further: the current bottom-bar tab, an
/// attachment chip, a quick-answer button under the pointer, a keycap.
pub const BAND2: u32 = 0x363638;
pub const BAND2_GLASS: u32 = 0xffffff1a;
/// `#2c2c2e` — a row or control under the pointer, on any ground.
pub const HOVER: u32 = 0x2c2c2e;
pub const HOVER_GLASS: u32 = 0xffffff0f;
/// `#343437` — the selected row: the focused Thread, the menu cursor, a
/// selected option.
pub const SELECTION: u32 = 0x343437;
pub const SELECTION_GLASS: u32 = 0xffffff1c;
/// `#3a3a3d` — a selected row under the pointer, and every press.
pub const SELECTION_HOVER: u32 = 0x3a3a3d;
pub const SELECTION_HOVER_GLASS: u32 = 0xffffff24;
/// `#2a2a2c` — the floating ground: menus, pickers, the palette, popovers,
/// tooltips, toasts, sheets.
pub const FLOAT: u32 = 0x2a2a2c;
/// A float as glass: near-opaque (96%). gpui blurs no single element, so a
/// float must stay legible on its own over a busy board.
pub const FLOAT_GLASS: u32 = 0x2c2c2ff5;
/// `#202022` — a diff's hunk header row.
#[allow(dead_code)]
pub const HUNK: u32 = 0x202022;
#[allow(dead_code)]
pub const HUNK_GLASS: u32 = 0x0000001f;
/// `#18181a` — the empty side of a split diff (a row with no partner).
#[allow(dead_code)]
pub const NODIFF: u32 = 0x18181a;
#[allow(dead_code)]
pub const NODIFF_GLASS: u32 = 0x00000033;

// ------------------------------------------------------------------ lines

/// `#2f2f31` — the one rule weight: the seam between Panes, the rule under
/// a Pane head, the bottom bar's top edge, a table's row rules, a thematic
/// break.
pub const LINE: u32 = 0x2f2f31;
pub const LINE_GLASS: u32 = 0xffffff14;
/// `#3c3c3f` — the stronger rule: a float's edge, a table's head rule, a
/// code block's left rail, an image's frame, a quick-answer button's edge.
pub const LINE2: u32 = 0x3c3c3f;
pub const LINE2_GLASS: u32 = 0xffffff24;
/// The column between the sidebar and the content: nothing on the opaque
/// greys (the value step separates them), a dark seam on glass.
#[allow(dead_code)]
pub const CHROME_SEAM_GLASS: u32 = 0x0000004d;
/// Fully transparent — an edge that is always in layout and only changes
/// colour, so nothing reflows when a Decision or a blocker arrives.
pub const TRANSPARENT: u32 = 0x00000000;

/// **The paints** (rule 4): what a region lays down, glass on macOS and
/// opaque grey elsewhere. Every fill and line is painted from here.
#[allow(dead_code)] // the builders' API: each paint is one region's
pub mod paint {
    use super::{solid, Paint, TRANSPARENT};

    /// The window's own root: nothing on glass, so each region's fill shows
    /// the blur behind the window; the reading plane elsewhere.
    pub const WINDOW: Paint = Paint::pick(TRANSPARENT, solid(super::PLANE));
    pub const PLANE: Paint = Paint::pick(super::PLANE_GLASS, solid(super::PLANE));
    pub const CHROME: Paint = Paint::pick(super::CHROME_GLASS, solid(super::CHROME));
    pub const HEAD: Paint = Paint::pick(super::HEAD_GLASS, solid(super::HEAD));
    pub const INBAND: Paint = Paint::pick(super::INBAND_GLASS, solid(super::INBAND));
    pub const BAND: Paint = Paint::pick(super::BAND_GLASS, solid(super::BAND));
    pub const BAND2: Paint = Paint::pick(super::BAND2_GLASS, solid(super::BAND2));
    pub const HOVER: Paint = Paint::pick(super::HOVER_GLASS, solid(super::HOVER));
    pub const SELECTION: Paint = Paint::pick(super::SELECTION_GLASS, solid(super::SELECTION));
    pub const SELECTION_HOVER: Paint =
        Paint::pick(super::SELECTION_HOVER_GLASS, solid(super::SELECTION_HOVER));
    /// Every press: the selected row's hover face.
    pub const PRESS: Paint = SELECTION_HOVER;
    pub const FLOAT: Paint = Paint::pick(super::FLOAT_GLASS, solid(super::FLOAT));
    pub const HUNK: Paint = Paint::pick(super::HUNK_GLASS, solid(super::HUNK));
    pub const NODIFF: Paint = Paint::pick(super::NODIFF_GLASS, solid(super::NODIFF));
    pub const LINE: Paint = Paint::pick(super::LINE_GLASS, solid(super::LINE));
    pub const LINE2: Paint = Paint::pick(super::LINE2_GLASS, solid(super::LINE2));
    pub const CHROME_SEAM: Paint = Paint::pick(super::CHROME_SEAM_GLASS, TRANSPARENT);
}

// ------------------------------------------------------------------ edges

/// `#7c7c82` — a resting checkbox, radio or switch boundary: solid and at
/// least 3:1 on the plane, chrome, a float and a band, so an unchecked
/// control never vanishes.
pub const INPUT_EDGE: u32 = 0x7c7c82;
/// `#3c3c3f` / `#55555a` — the scrollbar thumb, at rest and under the
/// pointer.
pub const SCROLLBAR: u32 = 0x3c3c3f;
pub const SCROLLBAR_HOVER: u32 = 0x55555a;
/// `#ffffff1a` — an unlit meter segment and the usage lines' tracks.
pub const METER_OFF: u32 = 0xffffff1a;
/// The veil behind a modal sheet: lighter over glass, which already dims.
pub const VEIL: u32 = if GLASS { 0x00000033 } else { 0x00000059 };

// -------------------------------------------------------------------- ink

/// `#f4f4f5` — titles, the operator's own prompt text, headings, a tool's
/// name, bold runs, a Decision's question.
pub const TEXT_STRONG: u32 = 0xf4f4f5;
/// `#d7d7d9` — agent prose, the body copy, a Thread row's title.
pub const TEXT: u32 = 0xd7d7d9;
/// `#98989d` — metadata: tool arguments and results, durations, hints,
/// timestamps, placeholders, state words in the sidebar. The floor for any
/// text that must be read.
pub const TEXT_MUTED: u32 = 0x98989d;
/// `#5e5e63` — structure, never words: the `·` seam, `└` elbows, rails,
/// disclosure glyphs, the parked ring.
pub const TEXT_FAINT: u32 = 0x5e5e63;

// --------------------------------------------------------------- palette
//
// Real terminal colour (rule 6): the named hues. Semantic tokens below
// point at them; render code names the semantic token.

pub const RED: u32 = 0xef8a80;
pub const GREEN: u32 = 0x93cf8c;
pub const YELLOW: u32 = 0xe6c47c;
pub const BLUE: u32 = 0x82b1f2;
pub const MAGENTA: u32 = 0xc59df0;
pub const CYAN: u32 = 0x78ccd0;
pub const ORANGE: u32 = 0xeda879;

// ----------------------------------------------------------------- accent

/// `#8eb1f0` — steel blue, the operator's colour: the prompt `❯`, the caret,
/// the focused Pane's border, links, the current row's `❯`, a selected
/// choice, a drop target.
pub const ACCENT: u32 = 0x8eb1f0;
/// `#b4cbf6` — the accent a step lighter: link hover, accent on a selection.
pub const ACCENT_HI: u32 = 0xb4cbf6;
/// The accent as a fill: the primary button, the selected effort chip, with
/// `ON_ACCENT` ink.
pub const ACCENT_STRONG: u32 = ACCENT;
/// A primary button under the pointer and held down.
pub const PRIMARY_HOVER: u32 = 0xa5c1f3;
pub const PRIMARY_ACTIVE: u32 = 0x7fa3e3;
/// `#111214` — ink on an accent fill.
pub const ON_ACCENT: u32 = 0x111214;
/// **The** keyboard-focus ink: the focused Pane's border, the kit's `ring`,
/// every focus outline. The accent itself.
pub const FOCUS_RING: u32 = ACCENT;
/// `#8eb1f066` — the accent as an outline that is not focus: a link's
/// underline, a drop target's edge.
pub const ACCENT_EDGE: u32 = 0x8eb1f066;
/// `#8eb1f024` (14%) — the accent as a ground: the slot a dragged Pane would
/// take. Never inline code, never a state.
pub const ACCENT_WASH: u32 = 0x8eb1f024;
/// `#8eb1f040` (25%) — native text selection, painted over glyphs.
pub const TEXT_SELECTION_WASH: u32 = 0x8eb1f040;
/// The caret: the soft block in the focused Composer.
pub const CARET: u32 = ACCENT;

// -------------------------------------------------------- state + signals

/// Green — live work: the running dot and braille spinner, `ok`, a passing
/// check, diff `+`.
pub const RUNNING: u32 = GREEN;
/// Yellow — a Decision: the dot, the `◆`, the kind word, the waiting Pane's
/// border.
pub const ATTENTION: u32 = YELLOW;
/// A waiting Pane's border on a board: yellow at 35%, so the one
/// answer-target cell at full `ATTENTION` stands out (the operator's Q6
/// ruling; the prototype's lone waiting Pane is that target).
pub const ATTENTION_EDGE: u32 = 0xe6c47c59;
/// Red — blocked or failed: the dot, the word `failed`, diff `−`.
pub const BLOCKED: u32 = RED;
/// A closed Pane's border on a board: red at 35%.
pub const BLOCKED_EDGE: u32 = 0xef8a8059;
/// Red as a one-line ground (a refused drop, a destructive control under
/// the pointer); never a multi-line wash (`DIFF_REMOVED_WASH`).
pub const BLOCKED_WASH: u32 = 0xef8a801f;
/// The idle/parked dot: the muted ink in a dot role.
pub const IDLE: u32 = TEXT_MUTED;
/// The permission mode word in the status line (`accept edits`, `plan`).
#[allow(dead_code)]
pub const MODE_INK: u32 = MAGENTA;

/// **The lexicon.** Every state word Ferrite prints about a Thread or an
/// agent, defined once next to the inks that colour them. State and value
/// words are always lowercase, even when they lead a row, and never carry an
/// em dash: a status line is `word · detail · project`, the `·` in
/// `TEXT_FAINT`. Render sites name a word from here, never a literal, so the
/// notifications, the subagent strip, the nav and the Pane heads cannot
/// drift apart. Only the lead word of a line takes a colour (`word_ink`).
pub mod words {
    /// An agent is stopped until the operator acts (`ATTENTION`).
    pub const NEEDS_YOU: &str = "needs you";
    /// What a waiting agent needs: a permission request.
    pub const APPROVAL: &str = "approval";
    /// What a waiting agent needs: an answer.
    pub const QUESTION: &str = "question";
    /// A turn finished cleanly.
    pub const DONE: &str = "done";
    /// A turn or an agent failed (`BLOCKED`).
    pub const FAILED: &str = "failed";
    /// Tests are failing (`BLOCKED`).
    pub const FAILING: &str = "failing";
    /// The operator (or the runtime) stopped the work.
    pub const INTERRUPTED: &str = "interrupted";
    /// Live work.
    pub const WORKING: &str = "working";
    /// Launched, not yet working.
    pub const STARTING: &str = "starting";
    /// Held by the operator.
    pub const PAUSED: &str = "paused";
    /// Parked: no Session in memory, one keystroke from coming back.
    pub const PARKED: &str = "parked";
    /// Ferrite cannot observe it.
    pub const UNAVAILABLE: &str = "unavailable";
    /// An answer is on its way to the provider.
    pub const SENDING: &str = "sending";
}

/// The ink a lexicon word wears when it leads a line: `needs you` is
/// `ATTENTION`, `failed`/`failing` are `BLOCKED`, every other word is
/// `TEXT_MUTED` — colour always means something needs you. (The sidebar
/// sets its words in `TEXT_MUTED` whatever they say: its dot carries the
/// colour, rule 7.)
pub fn word_ink(word: &str) -> u32 {
    match word {
        words::NEEDS_YOU => ATTENTION,
        words::FAILED | words::FAILING => BLOCKED,
        _ => TEXT_MUTED,
    }
}

/// **Mode words** (C17, rule 2.11.5). A permission mode id never renders
/// raw: the known ids read as Claude Code prints them (`accept edits`,
/// `bypass permissions`, `plan`), in lowercase like every value word.
pub fn known_mode(id: &str) -> Option<&'static str> {
    match id {
        "acceptEdits" => Some("accept edits"),
        "bypassPermissions" => Some("bypass permissions"),
        "plan" => Some("plan"),
        _ => None,
    }
}

/// The word a permission mode wears in the status line, or `None` at the
/// default (empty or `default`), which is hidden. An unknown id is split at
/// its camelCase humps and lowercased (`dontAsk` → `dont ask`), so no raw id
/// or capital ever shows.
pub fn mode_word(id: &str) -> Option<gpui::SharedString> {
    if id.is_empty() || id == "default" {
        return None;
    }
    if let Some(word) = known_mode(id) {
        return Some(word.into());
    }
    let mut word = String::with_capacity(id.len() + 4);
    for (index, ch) in id.chars().enumerate() {
        if ch.is_uppercase() && index > 0 && !word.ends_with(' ') {
            word.push(' ');
        }
        word.extend(ch.to_lowercase());
    }
    Some(word.into())
}

/// A provider's logomark in its own brand colour — Claude's clay, Codex's
/// green. Only the mark and its working spinner wear it: never a label, a
/// row or a state.
pub const PROVIDER_CODEX: u32 = 0x10a37f;
pub const PROVIDER_CLAUDE: u32 = 0xd97757;

// ------------------------------------------------------- transcript colour

/// Syntax in real terminal colour (rule 6). Keywords: magenta.
pub const SYN_KEYWORD: u32 = MAGENTA;
/// Function names: blue.
pub const SYN_FUNCTION: u32 = BLUE;
/// Type names: yellow.
pub const SYN_TYPE: u32 = YELLOW;
/// String literals: green.
pub const SYN_STRING: u32 = GREEN;
/// Number literals: orange.
pub const SYN_NUMBER: u32 = ORANGE;
/// Constants (`true`, `None`, `SCREAMING_CASE`): orange, with the numbers.
#[allow(dead_code)]
pub const SYN_CONST: u32 = ORANGE;
/// Comments are read, not decoration: `TEXT_MUTED`, set italic.
pub const SYN_COMMENT: u32 = TEXT_MUTED;
/// Punctuation.
pub const SYN_PUNCT: u32 = TEXT_MUTED;
/// Everything the highlighter leaves unclassed.
pub const SYN_PLAIN: u32 = TEXT;
/// Inline `code` in prose: cyan ink, no chip.
pub const INLINE_CODE: u32 = CYAN;
/// A file path, in prose or in a tool call's arguments: cyan, underlined
/// under the pointer.
#[allow(dead_code)]
pub const PATH_INK: u32 = CYAN;
/// A link's ink and its underline.
pub const LINK_INK: u32 = ACCENT;
/// The wash over the slot a dragged Pane would take.
pub const DROP_WASH: u32 = ACCENT_WASH;
/// A diff row's changed words: the row's hue at 24% (added) and 28%
/// (removed), over the row's own wash (`DIFF_ADDED_WASH`,
/// `DIFF_REMOVED_WASH`).
#[allow(dead_code)]
pub const DIFF_ADDED_WORD: u32 = 0x93cf8c3d;
#[allow(dead_code)]
pub const DIFF_REMOVED_WORD: u32 = 0xef8a8047;
/// A diff row's sign: the only hued glyph on the row.
#[allow(dead_code)]
pub const DIFF_ADDED_SIGN: u32 = GREEN;
#[allow(dead_code)]
pub const DIFF_REMOVED_SIGN: u32 = RED;
/// How far the working caption's shimmer lifts its colour toward white at
/// the crest (`components::shimmer`): Claude's clay crests near `#ffe1d3`.
#[allow(dead_code)]
pub const SHIMMER_LIFT: f32 = 0.75;

// ------------------------------------------------------------- the float
//
// **One cast shadow** (rule 2): a floating surface's, CSS `0 6px 14px` at
// 50%. gpui's blur is a gaussian σ, half a CSS blur. Nothing else casts
// and nothing is lit.

pub const SHADOW_FLOAT: u32 = 0x00000080;
pub const SHADOW_FLOAT_Y: f32 = 6.0;
pub const SHADOW_FLOAT_BLUR: f32 = 7.0;
/// 16px — a scroll fade: where a scrolled list meets a fixed edge, its
/// content dissolves into the ground over this run instead of being cut.
pub const SCROLL_FADE_H: f32 = SPACE_4;

// ------------------------------------------------------------------- type
//
// **One size per surface** (rule 1). The chrome — sidebar, titlebar, bottom
// bar, Pane heads, menus, pickers, the palette, sheets, toasts, tooltips,
// the Composer and its status line — is `FS_UI` 13 on `LH_UI` 20. The
// transcript is the reading size on its own line (`answer_text_size`,
// `answer_line_height`; 14 on 21 at Standard) and every transcript measure
// scales from it. Hierarchy is ink and weight, never a size: no surface
// sets a second size, and headings are the body size.
//
// **Weights:** `W_BODY` (400) for everything that is read; `W_LABEL` (500)
// a surface's one title, a tool's name, a label; `W_STRONG` (600) the
// prompt `❯`, headings, `**strong**`, a Project heading, a table head. 700
// is never used, and no render site names a `FontWeight` itself. Weight
// never signals state on a row that can truncate.
//
// **Figures:** any number that changes while it is on screen is
// `components::tabular` (Geist Mono's figures are tabular already; the
// feature keeps that true if the face changes).

/// 13px — the grid's size: every chrome line, on `LH_UI`.
pub const FS_UI: f32 = 13.0;
/// 20px — the grid's row.
pub const LH_UI: f32 = 20.0;
/// 14px — the Standard reading size (`ReadingSize::STANDARD`): agent prose
/// and every transcript row at the default zoom, on `LH_PROSE`.
pub const FS_PROSE: f32 = 14.0;
/// 21px — the Standard reading size's line: 1.5×.
pub const LH_PROSE: f32 = 21.0;

/// Weights (see the type notes above): 400 body; 500 a title, a tool's
/// name, a label; 600 the prompt mark, headings, `**strong**`.
pub const W_BODY: FontWeight = FontWeight::NORMAL;
pub const W_LABEL: FontWeight = FontWeight::MEDIUM;
pub const W_STRONG: FontWeight = FontWeight::SEMIBOLD;

/// The transcript's reading size (cmd-= / cmd-- / cmd-0, every Pane) is a
/// zoom over the whole transcript grid: the answer size is the setting's
/// own px.
pub fn answer_text_size(size: ferrite_core::settings::ReadingSize) -> f32 {
    f32::from(size.px())
}

/// The pixel line height paired with each reading size: 1.5× and whole, the
/// terminal's row (13 sits on the chrome's 20): 12/18 · 13/20 · 14/21 ·
/// 15/23 · 16/24 · 18/27 · 20/30 · 22/33 · 24/36.
pub fn answer_line_height(size: ferrite_core::settings::ReadingSize) -> f32 {
    match size.px() {
        13 => LH_UI,
        14 => LH_PROSE,
        px => (f32::from(px) * 1.5).round(),
    }
}

/// Headings are the body size (rule 1): they stand apart by weight and ink.
/// Kept as a function so the Markdown path has one place to ask.
pub fn heading_scale(_level: u8) -> f32 {
    1.
}

/// A prose size's pixel line height: `round(size × 1.5)`.
pub fn prose_line_height(size: f32) -> f32 {
    (size * LH_PROSE / FS_PROSE).round()
}

/// 0.6em — Geist Mono's advance width (600/1000 em): every glyph's cell.
pub const CODE_ADVANCE: f32 = 0.6;
/// 7.8px — one cell at `FS_UI`.
pub const CODE_CELL: f32 = FS_UI * CODE_ADVANCE;
/// One cell at the grid size: every horizontal metric on the grid is a
/// whole number of these (the 2-cell glyph gutter, a 36-cell sidebar).
#[allow(dead_code)]
pub const CH: f32 = CODE_CELL;
/// One row of the grid, and half of one: every vertical metric on the grid
/// is a whole number of half rows.
#[allow(dead_code)]
pub const ROW: f32 = LH_UI;
#[allow(dead_code)]
pub const HALF_ROW: f32 = LH_UI / 2.0;
/// The 2-cell glyph gutter the prototype hangs every mark in (`❯`, `●`,
/// `└`, the spinner) at the grid size; a transcript's gutter is two cells
/// at its own reading size.
#[allow(dead_code)]
pub const GLYPH_GUTTER: f32 = 2.0 * CH;
/// 108 cells — the prose measure: the most a paragraph, a code block or a
/// table runs before it wraps, at any reading size.
#[allow(dead_code)]
pub const MEASURE_CH: f32 = 108.0;
/// A floor under the face's advance for laying out against text width
/// estimates (a title's floor). With one monospace face every advance is
/// `CODE_ADVANCE`; this stays a floor so a short label is never padded.
pub const UI_ADVANCE_FLOOR: f32 = 0.5;
/// 4px — one word space between runs laid side by side.
pub const WORD_GAP: f32 = SPACE_1;

/// The non-ASCII glyphs text may use: every one is in Geist Mono's cmap
/// (asserted by `theme::tests`). Anything else is an SVG in a glyph box,
/// never text (`DRAWN_GLYPHS`). A rule the tests enforce, so it compiles
/// only with them.
#[cfg(test)]
pub const CHROME_GLYPHS: &[char] = &[
    '●', '○', '└', '├', '│', '─', '·', '•', '…', '↑', '↓', '←', '→', '↳', '↩', '⇧', '⇥', '⏎', '↵',
    '⌫', '±', '−', '—', '×', '›', '‹', '▲', '▼', '▶', '◀',
];
/// The glyphs the design uses that Geist Mono lacks: each is drawn (an SVG
/// in a glyph box, `icons.rs`), never typed. Asserted *absent* from the
/// face, so a face update that adds one is noticed.
#[cfg(test)]
pub const DRAWN_GLYPHS: &[char] = &[
    '❯', '✻', '◆', '⏵', '✓', '✗', '▾', '▸', '⌘', '⌥', '⌃', '⎿', '∴', '☐', '⎇', '■', '✢', '✳', '✶',
    '✽', '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏',
];

/// 720px — the reading column's maximum width (legacy: the terminal grammar
/// starts every row at the Pane's gutter and holds prose to `MEASURE_CH`).
pub const READING_MAX_W: f32 = 720.0;

// ------------------------------------------------------------------ space

/// The space scale, in gpui's 4px-unit names (`SPACE_2` = `.p_2()` = 8px).
pub const SPACE_0_5: f32 = 2.0;
pub const SPACE_1: f32 = 4.0;
pub const SPACE_1_5: f32 = 6.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_6: f32 = 24.0;
pub const SPACE_8: f32 = 32.0;

// ------------------------------------------------------------------ radii
//
// **Square inside** (rule 2): every in-app radius is 0. The names stay, so
// call sites compile and say what they are; a status dot stays round
// because it is a dot, not a corner.

/// A Pane, a sheet.
pub const R_PANE: f32 = 0.0;
/// Blocks: the Composer, code, menus, popovers, toasts. The kit's
/// `radius_lg`.
pub const R_BLOCK: f32 = 0.0;
/// Controls: buttons, fields, rows, pickers, tooltips. The kit's `radius`.
pub const R_CONTROL: f32 = 0.0;
/// Chips, keycaps, a menu's rows.
pub const R_CHIP: f32 = 0.0;
/// Meter segments and other tiny marks.
pub const R_TIGHT: f32 = 0.0;

// --------------------------------------------------------- shell and board

/// 286px — the navigation column. The collapsed rail is 77px on macOS so
/// it owns the same horizontal reserve as the native traffic-light group;
/// the project title therefore starts beyond the window controls. Other
/// platforms keep the conventional compact 56px rail.
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input.
pub const NAV_WIDTH: f32 = 286.0;
pub const NAV_RAIL_WIDTH: f32 = if cfg!(target_os = "macos") {
    TRAFFIC_RESERVE
} else {
    56.0
};
/// 32px — the window-chrome band across the top of the window, the
/// prototype's titlebar (`TITLEBAR_H`). Over the nav it is the column's own
/// chrome row (the traffic-light reserve and the collapse button,
/// `nav::win_chrome`); over the board it is the titlebar strip (location,
/// `dev`, add control; on Windows the caption buttons, `titlebar::strip`),
/// an overlay that adds no layout. The Pane board starts under it, at
/// `BOARD_TOP`; the host traffic lights centre in it (`TRAFFIC_Y`).
pub const WIN_CHROME_H: f32 = TITLEBAR_H;
/// 32px — the titlebar band, and `STATUS_BAR_H` 24 the bottom bar (one
/// `ROW` and 4px, closed above by a `LINE`).
pub const TITLEBAR_H: f32 = 32.0;
#[allow(dead_code)]
pub const STATUS_BAR_H: f32 = ROW + SPACE_1;
/// 77px — the horizontal room the window-chrome band reserves before the
/// collapse button: the traffic lights plus the prototype's 8px flex gap
/// and 4px button margin. Measured from the prototype (button left edge
/// x = 77). On macOS the *host* lights occupy it; nothing else may be drawn
/// or hit-testable there, or AppKit's native drag region stops working.
pub const TRAFFIC_RESERVE: f32 = 77.0;
/// Where the host traffic-light group's close button sits: 13px in from the
/// window's left edge, vertically centred for a 14px button in the band.
pub const TRAFFIC_X: f32 = 13.0;
pub const TRAFFIC_Y: f32 = (WIN_CHROME_H - 14.0) / 2.0;
/// The Pane board: 8px gap on both axes, 10px padding on all four sides.
/// (The prototype's own render reserves 58px at the bottom for its
/// mode-switcher; that is prototype-only chrome and its `data-view="window"`
/// rule restores 10px. Port 10px.)
pub const GRID_GAP: f32 = ferrite_core::layout::GRID_GAP;
pub const GRID_PAD: f32 = 10.0;
/// Where the board starts: under the titlebar band, then the same 10px it
/// keeps on its other three sides. Flush to the band, a Pane's top-right
/// corner sits directly beneath the caption buttons, and their hover face
/// — edge-to-edge by design — reads as lying over the Pane.
pub const BOARD_TOP: f32 = WIN_CHROME_H + GRID_PAD;
/// A toast's width: the nav column less 8px each side.
pub const TOAST_W: f32 = NAV_WIDTH - 2.0 * SPACE_2;
/// The gap the kit fans a stack out by under the pointer. The stack holds
/// one toast (`max_items`), so this is only ever the kit's own spacing.
pub const TOAST_GAP: f32 = SPACE_3;

// -------------------------------------------- pane body and the row column

/// 16px — the inline padding every Pane strip shares.
pub const PANE_PAD_X: f32 = SPACE_4;
/// The Pane body's padding: 16px top, so the first line never kisses the
/// head rule, and at the bottom the room the working line overlays —
/// `GAP_BLOCK` above the line, the line, `GAP_ROW` under it (36px) — so the
/// last row sits 12px above the working line, the line 4px above the
/// Composer, and starting or stopping a turn reflows nothing.
pub const BODY_PAD_T: f32 = SPACE_4;
pub const BODY_PAD_B: f32 = GAP_BLOCK + LH_UI + GAP_ROW;
/// 12px — the glyph box every transcript and Composer row hangs its mark in
/// (`❯`, a tool dot, the answer mark, an elbow).
pub const GLYPH_BOX: f32 = 12.0;
/// 8px — from the glyph box to the row's text.
pub const GUTTER_GAP: f32 = 8.0;
/// 20px — **C1**, the text column of every transcript and Composer row:
/// `GLYPH_BOX + GUTTER_GAP`. An elbow result sits at C2 = C1 + `ELBOW_INDENT`.
/// (On the grid the gutter is two cells, `GLYPH_GUTTER`.)
pub const GUTTER_W: f32 = GLYPH_BOX + GUTTER_GAP;
/// C2 − C1: an elbow row indents by one gutter.
pub const ELBOW_INDENT: f32 = GUTTER_W;
/// 13px — a raised box's content inset (1px edge + 12px padding). Transcript
/// rows sit the same distance inside the reading column, so the transcript `❯`
/// and the Composer `❯` share one axis.
pub const BOX_INSET_X: f32 = 13.0;
/// 8px — between a row's glyph column and its text (tool rows, the working
/// line, the turn diff, controls beside a label).
pub const EVENT_GAP: f32 = 8.0;
/// 1px — the focused Pane's border (`FOCUS_RING` ink), lying exactly on the
/// Pane's own border box: focus changes colour and nothing else. Every
/// control's keyboard focus is the same 1px of the same ink, inset
/// (`components::control_focus`).
pub const FOCUS_RING_W: f32 = 1.0;

// ------------------------------------------------ shared controls and rows

/// 28px — an icon button, a rail item and the Project filter trigger.
pub const ICON_BUTTON: f32 = 28.0;
/// 16px — an icon button's glyph, centred in `ICON_BUTTON`.
pub const ICON_BUTTON_GLYPH: f32 = 16.0;
/// 12px — the chevron beside a picker or a disclosure.
pub const ICON_CHEVRON: f32 = 12.0;
/// 28px — a text button in pane and nav chrome, decisions and footers, with
/// 12px inline padding. Sheet controls are `FORM_CONTROL_H`.
pub const CONTROL_H: f32 = 28.0;
pub const CONTROL_PAD_X: f32 = SPACE_3;
/// 10px — the drawn `⌘` in a key combination (`components::key_combo`).
pub const KEY_GLYPH: f32 = 10.0;
/// A keycap: 18px high (it fits inside a 20px row), 5px inline padding.
pub const KBD_H: f32 = 18.0;
pub const KBD_PAD_X: f32 = 5.0;
/// A chip: 20px high (one row), 6px inline padding.
pub const CHIP_H: f32 = 20.0;
pub const CHIP_PAD_X: f32 = 6.0;
/// 4px — a floating menu's inset around its rows (`FLOAT_PAD`).
pub const MENU_PAD: f32 = 4.0;
/// 28px — one menu row: `LH_UI` plus 4px above and below.
pub const MENU_ROW_H: f32 = 28.0;
/// A floating surface's inset around its rows (the same 4px as `MENU_PAD`),
/// and the gap it keeps from the control that opened it.
pub const FLOAT_PAD: f32 = MENU_PAD;
pub const FLOAT_OFFSET: f32 = 6.0;
/// The context menu's width, and any floating list's height cap before it
/// scrolls.
pub const MENU_W: f32 = 256.0;
pub const MENU_MAX_H: f32 = 420.0;
/// A menu row's inline padding and the gap between its label and trailing
/// parts.
pub const MENU_ROW_PAD_X: f32 = 8.0;
pub const MENU_ROW_GAP: f32 = SPACE_3;
pub const R_MENU_ROW: f32 = R_CHIP;
/// A menu section title row: 24px, its title sat on the row's foot so it
/// hugs the rows it heads.
pub const MENU_SECTION_H: f32 = 24.0;
/// 10px — a section title's mark.
pub const MENU_SECTION_ICON: f32 = KEY_GLYPH;
/// 8px — what splits one group of rows from the next inside any floating
/// surface. Space, never a rule (`components::menu_separator`).
pub const MENU_GROUP_GAP: f32 = SPACE_2;
/// An aligned name column (slash commands): clamped between these.
pub const MENU_NAME_MIN_W: f32 = 96.0;
pub const MENU_NAME_MAX_W: f32 = 220.0;
/// A list row's padding — 8px inline, 6px block (the nav's section
/// headings, its empty state and its refusal notice).
pub const ROW_PAD_X: f32 = 8.0;
pub const ROW_PAD_Y: f32 = 6.0;
/// 4px — a nav row's block padding around its one `LH_UI` line.
pub const NAV_ROW_PAD_Y: f32 = SPACE_1;
/// 28px — **the** list pitch (C9): one `FS_UI`/`LH_UI` line with 4px above
/// and below, the same box as a menu row (`MENU_ROW_H`). Derived from the
/// type, never summed by hand.
pub const NAV_ROW_H: f32 = 2.0 * NAV_ROW_PAD_Y + LH_UI;
/// A Thread row: the one 28px line.
pub const THREAD_ROW_H: f32 = NAV_ROW_H;
/// A Group parent row: the same 28px line.
pub const GROUP_ROW_H: f32 = NAV_ROW_H;
/// The folder and branch marks on the Project and checkout lines (12px), and
/// the 5px gap to their labels.
pub const ROW_ICON: f32 = 12.0;
pub const ROW_ICON_GAP: f32 = 5.0;
/// 12px — the provider logomark in a picker row and the Composer's chip.
pub const PROVIDER_MARK_SM: f32 = 12.0;
/// 20px — one queued prompt's row pitch in the Composer's queue viewport.
pub const QUEUE_ROW_H: f32 = COMPOSER_ROW_H;

// ------------------------------------------------------- status and motion

/// 6px — the painted status dot (`components::status_dot`).
pub const STATUS_DOT: f32 = 6.0;
/// The dimmest a breath goes (legacy: no dot breathes now, rule 8).
pub const PULSE_MIN: f32 = 0.15;
/// The Ferrite progress mark follows the timing and geometry of the supplied
/// animated logo. These are artwork tokens rather than general motion tokens:
/// the SVG's 1254-unit viewBox is the coordinate system behind both offsets.
pub const FERRITE_SNAP_MS: u64 = 3_000;
pub const FERRITE_SHARD_X: f32 = 38.0 / 1254.0;
pub const FERRITE_SHARD_Y: f32 = 46.0 / 1254.0;
pub const FERRITE_PULL_START: f32 = 0.12;
pub const FERRITE_PULL_END: f32 = 0.34;
pub const FERRITE_HOLD_END: f32 = 0.54;
pub const FERRITE_SNAP_END: f32 = 0.615;
pub const FERRITE_PULL_EASING: [f32; 4] = [0.4, 0.0, 0.2, 1.0];
pub const FERRITE_SNAP_EASING: [f32; 4] = [0.16, 1.0, 0.3, 1.0];

// ------------------------------------------------------------------ faces

/// The face, **bundled**: Geist Mono, on every surface (rule 1). The kit's
/// `font_family` is set from it.
///
/// gpui has no variation-axis support, so `main.rs` registers static
/// instances (Regular, Italic, Medium, SemiBold, Bold). They share their
/// typographic family name (name ID 16), and CoreText/DirectWrite resolve
/// the face from `.font_weight(..)`. **Never reach a weight by family
/// name**: `.font_family("Geist Mono Medium")` silently resolves to the
/// fallback face.
pub const FONT_UI: &str = "Geist Mono";

/// The same face, named for what the text is (code, machine output, keys):
/// the kit's `mono_font_family`.
pub const FONT_CODE: &str = FONT_UI;

/// How the window's own background is drawn (rule 4): blurred glass on
/// macOS, opaque elsewhere. The root then paints `paint::WINDOW`.
pub fn window_background() -> gpui::WindowBackgroundAppearance {
    if GLASS {
        gpui::WindowBackgroundAppearance::Blurred
    } else {
        gpui::WindowBackgroundAppearance::Opaque
    }
}

/// Install Longbridge once per app, then map its semantic theme to Ferrite's
/// tokens. Constructors also call this for standalone test windows. The
/// window mounts the toolkit Root for Settings search input and focus.
pub fn init_components(cx: &mut gpui::App) {
    use gpui::component::{Theme, ThemeMode};
    use gpui::{px, rgb, rgba};

    let defaults = gpui::base::text::TextViewDefaults::global(cx);
    if !defaults.has_code_block_highlighter() {
        // GPUI caches highlighting by callback identity. Install it once so
        // unrelated pane renders do not highlight completed code again.
        defaults
            .with_code_block_highlighter(|block| {
                let source = block.code();
                let language = block.lang();
                let tokens =
                    ferrite_core::transcript::highlight_tokens(language.as_deref(), &source);
                crate::pane::code(&source, Some(&tokens))
            })
            .install(cx);
    }
    if cx.has_global::<Theme>() {
        return;
    }
    gpui::component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    crate::attachments::init(cx);
    crate::rich::init(cx);
    let theme = Theme::global_mut(cx);
    // One face, one size (rule 1).
    theme.font_family = FONT_UI.into();
    theme.font_size = px(FS_UI);
    theme.mono_font_family = FONT_CODE.into();
    theme.mono_font_size = px(FS_UI);
    // Square inside (rule 2).
    theme.radius = px(R_CONTROL);
    theme.radius_lg = px(R_BLOCK);
    // Floats carry Ferrite's one float shadow; the kit's own stays off.
    theme.shadow = false;
    theme.motion.spring_move = gpui::base::Spring::new(std::time::Duration::from_millis(120))
        .with_damping(1.0)
        .with_epsilon(0.1);
    // The kit's ground is the opaque plane: its fields and dialogs sit on
    // a region already painted, never on the bare glass window (the window
    // Root's own ground is overridden with `paint::WINDOW`, `main.rs`).
    theme.background = rgb(PLANE).into();
    theme.foreground = rgb(TEXT).into();
    theme.border = paint::LINE2.into();
    // Kit menu and completion rows sit on a float: their hover and cursor
    // face is the selection overlay.
    theme.accent = paint::SELECTION.into();
    theme.accent_foreground = rgb(TEXT_STRONG).into();
    theme.secondary = rgb(BAND).into();
    theme.secondary_hover = rgb(SELECTION).into();
    theme.secondary_active = rgb(SELECTION_HOVER).into();
    theme.secondary_foreground = rgb(TEXT).into();
    theme.primary = rgb(ACCENT_STRONG).into();
    theme.primary_hover = rgb(PRIMARY_HOVER).into();
    theme.primary_active = rgb(PRIMARY_ACTIVE).into();
    theme.primary_foreground = rgb(ON_ACCENT).into();
    // `Button::primary` reads its own fields, not `primary`.
    theme.button_primary = rgb(ACCENT_STRONG).into();
    theme.button_primary_hover = rgb(PRIMARY_HOVER).into();
    theme.button_primary_active = rgb(PRIMARY_ACTIVE).into();
    theme.button_primary_foreground = rgb(ON_ACCENT).into();
    theme.muted = rgb(BAND).into();
    theme.muted_foreground = rgb(TEXT_MUTED).into();
    theme.popover = paint::FLOAT.into();
    theme.popover_foreground = rgb(TEXT).into();
    theme.ring = rgb(FOCUS_RING).into();
    theme.caret = rgb(CARET).into();
    // The reader's line numbers sit on the plane itself. The gutter is
    // painted opaque over the text, so it takes the plane's own colour.
    let mut highlight = (*theme.highlight_theme).clone();
    highlight.style.editor_gutter_background = Some(rgb(PLANE).into());
    theme.highlight_theme = std::sync::Arc::new(highlight);
    theme.selection = rgba(TEXT_SELECTION_WASH).into();
    theme.link = rgb(LINK_INK).into();
    theme.link_hover = rgb(ACCENT_HI).into();
    theme.link_active = rgb(LINK_INK).into();
    // Native checkbox/radio indicators use `input` for their resting edge.
    theme.input = rgb(INPUT_EDGE).into();
    theme.switch = rgb(BAND2).into();
    theme.switch_thumb = rgb(TEXT_STRONG).into();
    theme.overlay = rgba(VEIL).into();
    theme.danger = rgb(BLOCKED).into();
    theme.warning = rgb(ATTENTION).into();
    theme.success = rgb(RUNNING).into();
    theme.info = rgb(ACCENT).into();
    theme.list_hover = paint::HOVER.into();
    theme.list_active = paint::SELECTION.into();
    theme.table_head = rgb(PLANE).into();
    theme.table_head_foreground = rgb(TEXT_STRONG).into();
    theme.drag_border = rgb(ACCENT).into();
    theme.drop_target = rgba(ACCENT_WASH).into();
    theme.sidebar = rgb(CHROME).into();
    theme.sidebar_foreground = rgb(TEXT).into();
    // The selected Settings page is the selection row.
    theme.sidebar_accent = rgb(SELECTION).into();
    theme.sidebar_accent_foreground = rgb(TEXT_STRONG).into();
    theme.sidebar_border = rgba(TRANSPARENT).into();
    // No track: only the thumb is ever ink, and it lightens rather than
    // darkens when the pointer takes hold of it.
    theme.scrollbar = rgba(TRANSPARENT).into();
    theme.scrollbar_thumb = rgb(SCROLLBAR).into();
    theme.scrollbar_thumb_hover = rgb(SCROLLBAR_HOVER).into();
    // Kit tabs, should one appear: no bar ground, the active tab `BAND2` in
    // `TEXT_STRONG` (the bottom bar's current tab), the rest muted.
    theme.tab_bar = rgba(TRANSPARENT).into();
    theme.tab_active = rgb(BAND2).into();
    theme.tab_active_foreground = rgb(TEXT_STRONG).into();
    theme.tab_foreground = rgb(TEXT_MUTED).into();
    // `Theme::change` resolved the kit's pre-computed tokens from its default
    // palette, and nothing recomputes them: widgets that read `tokens.*`
    // (Button::primary, menu rows, tooltips, checkboxes) would paint the
    // kit's neutrals. Rebuild them from the colours above.
    theme.tokens = gpui::component::ThemeTokens::from(&theme.colors);
    // A toast is the rail's voice only (C8): with the nav open the
    // Needs-you strip is the queue, and a toast shows only while the nav is
    // collapsed and its Thread is off the board (`Bell::present`). It
    // stands BottomRight, above the Composer, one at a time; the bell
    // holds the rest.
    theme.notification.placement = gpui::Anchor::BottomRight;
    theme.notification.margins = gpui::base::Edges {
        top: px(BOARD_TOP),
        right: px(GRID_PAD),
        bottom: px(TOAST_ABOVE_COMPOSER),
        left: px(SPACE_2),
    };
    theme.notification.width = px(TOAST_W);
    theme.notification.max_items = 1;
    // Only the front toast shows: no ghost edges behind it.
    cx.set_global(gpui::base::DefaultToastMotion(gpui::base::ToastMotion {
        collapsed_peek: px(0.),
        collapsed_scale_step: 0.,
        collapsed_visible: 1,
        expanded_gap: px(TOAST_GAP),
        // The stack settles over the toast tokens (rule 2.10.4).
        duration: std::time::Duration::from_millis(MOTION_TOAST_IN_MS),
        exit_duration: std::time::Duration::from_millis(MOTION_TOAST_OUT_MS),
    }));
    // C26: the kit's own scrollbars (every `overflow_y_scrollbar` site) keep
    // Ferrite's timing — on the first scroll frame, a 1.4s hold, a 150ms
    // fade — so they match `scrollbar.rs`.
    let base = gpui::base::Theme::global_mut(cx);
    base.scrollbar = base.scrollbar.clone().with_motion(
        gpui::base::ScrollbarMotion::default()
            .with_enter(std::time::Duration::ZERO)
            .with_idle(std::time::Duration::from_millis(MOTION_SCROLLBAR_LINGER_MS))
            .with_exit(std::time::Duration::from_millis(MOTION_HOVER_FADE_MS)),
    );
}

// ======================================== end of the frozen shared head

// ======================================== legacy aliases — remove after integration
// The previous design's names, mapped onto the tokens above so every module
// builds while the builders restyle their surfaces. New code names the
// tokens above (`paint::*` for fills and lines); each alias says what
// replaces it. Once no call site names one, delete it.

pub use legacy::*;

#[allow(dead_code)]
mod legacy {
    use super::*;

    /// → `CHROME` (the sidebar) / `paint::WINDOW` (the window root). The old
    /// window ground.
    pub const GROUND: u32 = CHROME;
    /// → `CHROME` / `paint::CHROME`.
    pub const NAV: u32 = CHROME;
    /// → `PLANE` / `paint::PLANE`.
    pub const PANE: u32 = PLANE;
    /// → `BAND` / `paint::BAND` (a band) or nothing (code blocks and cards
    /// have no ground now).
    pub const RAISED: u32 = BAND;
    /// → `FLOAT` / `paint::FLOAT`.
    pub const MENU: u32 = FLOAT;
    /// → `BAND2` / `paint::BAND2`.
    pub const RAISED_2: u32 = BAND2;
    /// → `HOVER` / `paint::HOVER`: one hover face on every ground.
    pub const HOVER_RAISED: u32 = HOVER;
    /// → `SELECTION` / `paint::SELECTION`.
    pub const FILL: u32 = SELECTION;
    /// → `SELECTION_HOVER` / `paint::SELECTION_HOVER`.
    pub const FILL_HOVER: u32 = SELECTION_HOVER;
    /// → `paint::PRESS`.
    pub const PRESSED: u32 = SELECTION_HOVER;
    /// → `paint::LINE` (already the platform's `0xRRGGBBAA`).
    pub const HAIRLINE: u32 = paint::LINE.0;
    /// → `paint::LINE2`.
    pub const HAIRLINE_STRONG: u32 = paint::LINE2.0;
    /// → `paint::LINE2` (the Composer has no box now; its input is a band).
    pub const COMPOSER_EDGE: u32 = HAIRLINE_STRONG;
    /// → `TEXT` or `TEXT_MUTED`: the ladder has four inks now. Held between
    /// them so nothing collapses before its surface is restyled.
    pub const TEXT_2: u32 = 0xb4b4b8;
    /// → `FS_UI`: one size per surface.
    pub const FS_SM: f32 = FS_UI;
    /// → `FS_UI`.
    pub const FS_PROSE_SM: f32 = FS_UI;
    /// → `LH_UI`.
    pub const LH_META: f32 = LH_UI;
    /// → `LH_UI`.
    pub const LH_PROSE_SM: f32 = LH_UI;
    /// → `LH_UI` (code sits on the same grid).
    pub const LH_CODE: f32 = LH_UI;
    /// Nothing is lit (rule 2): the old top lights, a keycap's foot and a
    /// well's lip paint nothing.
    pub const LIGHT_LOW: u32 = TRANSPARENT;
    pub const LIGHT_HIGH: u32 = TRANSPARENT;
    pub const KEY_FOOT: u32 = TRANSPARENT;
    pub const WELL_SHADE: u32 = TRANSPARENT;
    /// Nothing casts but a float (rule 2): the old ladder's layers, kept as
    /// names only. `components::elevation` no longer reads them.
    pub const SHADOW_CONTACT: u32 = TRANSPARENT;
    pub const SHADOW_CONTACT_Y: f32 = 0.0;
    pub const SHADOW_CONTACT_BLUR: f32 = 0.0;
    pub const SHADOW_RAISED: u32 = TRANSPARENT;
    pub const SHADOW_RAISED_Y: f32 = 0.0;
    pub const SHADOW_RAISED_BLUR: f32 = 0.0;
    pub const SHADOW_RAISED_SPREAD: f32 = 0.0;
    pub const SHADOW_PANE: u32 = TRANSPARENT;
    pub const SHADOW_PANE_Y: f32 = 0.0;
    pub const SHADOW_PANE_BLUR: f32 = 0.0;
    pub const SHADOW_PANE_SPREAD: f32 = 0.0;
    pub const SHADOW_LIFTED: u32 = TRANSPARENT;
    pub const SHADOW_LIFTED_Y: f32 = 0.0;
    pub const SHADOW_LIFTED_BLUR: f32 = 0.0;
    pub const SHADOW_LIFTED_SPREAD: f32 = 0.0;
    pub const SHADOW_NEAR: u32 = SHADOW_FLOAT;
    pub const SHADOW_NEAR_Y: f32 = SHADOW_FLOAT_Y;
    pub const SHADOW_NEAR_BLUR: f32 = SHADOW_FLOAT_BLUR;
    pub const SHADOW_NEAR_SPREAD: f32 = 0.0;
    pub const SHADOW_FAR: u32 = TRANSPARENT;
    pub const SHADOW_FAR_Y: f32 = 0.0;
    pub const SHADOW_FAR_BLUR: f32 = 0.0;
    pub const SHADOW_FAR_SPREAD: f32 = 0.0;
    pub const SHADOW_SHEET: u32 = TRANSPARENT;
    pub const SHADOW_SHEET_Y: f32 = 0.0;
    pub const SHADOW_SHEET_BLUR: f32 = 0.0;
    pub const SHADOW_SHEET_SPREAD: f32 = 0.0;
}
// (end legacy aliases)

// ======================================== WP-A · transcript rows and grammar
// Owner: WP-A (transcript.rs, pane/text.rs, the transcript rows in pane.rs, ferrite-core transcript strings.)
// Edit values and append tokens only inside this section.

// ---------------------------------------------------- transcript grammar
//
// The transcript is a terminal: Claude Code's own layout on one monospace
// grid, in Ferrite's ink (the prototype's `.r`, `.prompt`, `.line`, `.out`).
//
// - **One grid.** Every transcript row is set at the operator's reading
//   size on its 1.5x line (`answer_text_size`, `answer_line_height`), set
//   once on the transcript list and inherited by every row: prose, tool
//   calls, results, diffs, the stamp. Horizontal measures are cells at that
//   size (`tx_cell`); vertical ones are its line and half line.
// - **The 2-cell gutter.** Every row is `[gutter | content]`: the mark in
//   the gutter's first cell (`tx_gutter`, two cells wide), the content
//   column after it, and wrapped lines hang under the content column, never
//   under the mark. Rows sit `TX_PAD_L` (two chrome cells) from the Pane's
//   left edge and `TX_PAD_R` from its right; there is no centred column.
// - **Marks.** `❯` (accent, drawn: `icons::PROMPT`) heads the operator's
//   prompt; a typed `●` heads agent prose (`TEXT_STRONG`, once per speaker
//   change: `transcript::AnswerMarks`) and every tool call (`TEXT_MUTED`
//   done, `RUNNING` live, `BLOCKED` failed; never pulsing); `✻` (drawn,
//   `icons::WORKED`, in the provider's colour) the turn's end; `◆` (drawn,
//   `ATTENTION`) a Decision. `∴` (drawn) heads reasoning.
// - **The prompt band.** The prompt echo is a full-width `paint::BAND` with
//   half a line above and below its text: `❯` in the gutter, the text in
//   `TEXT_STRONG` at body weight. The band of the turn being read stays
//   pinned at the top of the body while its output scrolls under it (a
//   plain echo laid over the list, pushed up by the next turn's band).
// - **Tool rows.** `● Name(args)`: the name `TEXT_STRONG` at `W_LABEL`, the
//   parens and arguments `TEXT_MUTED`, one line that truncates; the trail
//   hard right (`+N −M` in `RUNNING`/`BLOCKED`, then the duration
//   `TEXT_MUTED`, tabular). What it produced hangs under a typed `└ ` elbow
//   (`TEXT_FAINT`) on the content column, in `TEXT_MUTED`; further output
//   lines align after the elbow. Long output folds to
//   `OUTPUT_PREVIEW_LINES` and a `+ N lines` line that toggles it.
// - **Rhythm in rows.** Blocks are one blank line apart (a turn's prompt
//   band, prose, a run of tool calls, the stamp); the calls of one run and
//   the rows that hang on an elbow under the row they answer sit flush.
//   The space above a row is chosen at reconcile from the row before it
//   (`rows::gap_before`) and is part of the row's identity.
// - **The turn's end.** `✻ Worked for 41s · 7:32 pm`, `TEXT_MUTED`, the `·`
//   seams `TEXT_FAINT`. A failed or interrupted turn hangs under the last
//   row: `└ failed · 0.1s · <message>`, the lead word in its state ink.
// - **Diffs** have no box: a `TEXT_MUTED` hunk header, `TEXT_MUTED` line
//   numbers, red and green row washes (`DIFF_*_WASH`) with the changed
//   words washed deeper (`DIFF_*_WORD`), code in full syntax colour, the
//   sign carrying the hue. Side by side once the transcript is
//   `SPLIT_DIFF_MIN_W` wide, unified when narrower; a split row's empty
//   side is `paint::NODIFF`.
// - **No transcript row has a hover ground** but the keyboard's disclosure
//   target (`paint::HOVER`); a disclosure's chevron shows under the pointer.

/// 66px — the tallest remnant of a cut row the transcript hides under its
/// top edge while it follows the tail (three prose lines): a prompt, a tool
/// row or a short paragraph cut under the head rule goes whole, so the
/// body reads from a whole row; a long block read mid-way stays, since
/// hiding more would open a void (rule 2.3.4).
pub const TRANSCRIPT_TOP_SNAP_MAX: f32 = 3.0 * LH_PROSE;
/// 32px — above every prompt but the first: the turn boundary. No rule is
/// drawn between turns; this space, the prompt's weight and the stamp do the
/// job.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const GAP_TURN: f32 = SPACE_8;
/// 12px — the block step: between the blocks of one turn (prompt → the
/// agent's first row, prose ↔ tools, anything ↔ reasoning, notices, the
/// turn's changes, the last block → its stamp).
pub const GAP_BLOCK: f32 = SPACE_3;
/// 4px — the row step: rows of one run of work, and a row hung on an elbow
/// under the row it answers.
pub const GAP_ROW: f32 = SPACE_1;

/// A prose-relative vertical step at answer size `size`: em-proportional to
/// the Standard prose size, whole pixels. `GAP_TURN`, `GAP_BLOCK` and the
/// Markdown gaps go through it; UI-row steps do not.
#[allow(dead_code)] // the previous grammar; remove after integration
pub fn reading_step(step: f32, size: f32) -> f32 {
    (step * size / FS_PROSE).round()
}

/// 6px — a tool call's state dot, the size of every status dot.
pub const TOOL_DOT: f32 = STATUS_DOT;
/// The elbow `⎿`, painted in the 12px glyph box: its stem 3px in, so it
/// stands under the stem of the call name's first letter, running from the
/// top of the row box to the first line's centre, then 8px along it.
pub const ELBOW_STEM_X: f32 = 3.0;
pub const ELBOW_ARM: f32 = SPACE_2;
/// A disclosed call echoes its input under `⎿` only when the call line could
/// not show it whole: a command (always, exactly), a titled call, a
/// multi-line input, or one longer than this many characters.
pub const INPUT_ECHO_CHARS: usize = 48;
/// A settled call shows its time only from one second up; anything quicker
/// is noise on every row.
pub const DURATION_MIN_MS: u128 = 1_000;
/// Output up to `OUTPUT_INLINE_BYTES` draws inline under its elbow, where a
/// copy sweep across the transcript reaches it; larger output scrolls in a
/// bounded native viewport `OUTPUT_MAX_LINES` high, with `… +N lines` under
/// it saying how much is out of view.
pub const OUTPUT_MAX_LINES: usize = 12;
pub const OUTPUT_INLINE_BYTES: usize = 8 * 1024;

/// A diff line's code ink when it carries no syntax colour: body ink on
/// its wash (the sign, `DIFF_*_SIGN`, carries the hue; rule 6).
pub const DIFF_ADDED_INK: u32 = TEXT;
pub const DIFF_REMOVED_INK: u32 = TEXT;
/// A diff row's wash: green at 10%, red at 11% (the prototype's `.h.ad`,
/// `.h.rm`); the changed words wash deeper (`DIFF_*_WORD`, in the head).
/// (`BLOCKED_WASH` is for one-line uses only.)
pub const DIFF_ADDED_WASH: u32 = 0x93cf8c1a;
pub const DIFF_REMOVED_WASH: u32 = 0xef8a801c;
/// The previous grammar's diff card (`RAISED`, padded, rounded): unused by
/// the terminal grammar, whose diff has no box and measures in cells
/// (`DIFF_NUMBER_CELLS`, `DIFF_SIGN_CELLS`).
#[allow(dead_code)] // the previous grammar; remove after integration
pub const HUNK_PAD_X: f32 = CODE_PAD_X;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const HUNK_PAD_Y: f32 = SPACE_1;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const HUNK_MARGIN_T: f32 = SPACE_1;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const DIFF_SIGN_W: f32 = SPACE_2;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const DIFF_GAP: f32 = SPACE_2;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const DIFF_SIGN_GAP: f32 = SPACE_1;
/// How many rows one hunk card draws before it stops and says how many it
/// did not. An edit's patch is a handful of lines; a written file's is
/// however long the file is, and a card that redrew a 900-line file would
/// be the transcript rather than a note in it.
pub const HUNK_MAX_ROWS: usize = 24;

/// 20px — an invisible hit area, not a drawn thing: a disclosure's gutter
/// target (the whole row toggles too) and a prompt action's button.
pub const TOOL_DISCLOSURE_HIT: f32 = GUTTER_W;
/// One cell of the transcript grid at reading size `size`: Geist Mono's
/// advance (`CODE_ADVANCE`), 8.4px at Standard.
pub fn tx_cell(size: f32) -> f32 {
    size * CODE_ADVANCE
}

/// The transcript's 2-cell glyph gutter at reading size `size`: the row's
/// mark in its first cell, the content column after its second.
pub fn tx_gutter(size: f32) -> f32 {
    2.0 * tx_cell(size)
}

/// A drawn transcript mark (`❯ ✻ ◆ ∴`) at reading size `size`: sized like
/// a glyph of the face, 12px at Standard.
pub fn tx_mark(size: f32) -> f32 {
    (size * 6.0 / 7.0).round()
}

/// Half a transcript line, whole pixels: the prompt band's padding above
/// and below its text.
pub fn tx_half(line: f32) -> f32 {
    (line / 2.0).floor()
}

/// The transcript's inset in the Pane body: two chrome cells on the left
/// (the prototype's `.scroll{padding:0 3ch 0 2ch}`), so the transcript `❯`
/// and the Composer's share one axis whatever the reading size, and three
/// on the right, clear of the minimap rail.
pub const TX_PAD_L: f32 = GLYPH_GUTTER;
pub const TX_PAD_R: f32 = 3.0 * CH;
/// Long tool output folds to this many lines under its elbow, then a
/// `+ N lines` line that unfolds it in place.
pub const OUTPUT_PREVIEW_LINES: usize = 3;
/// A diff goes side by side once the transcript is this wide (the
/// prototype's `@container (min-width: 1000px)`), unified when narrower.
pub const SPLIT_DIFF_MIN_W: f32 = 1000.0;
/// A diff's line-number column, in cells (its 1-cell pad included), and its
/// sign column.
pub const DIFF_NUMBER_CELLS: f32 = 5.0;
pub const DIFF_SIGN_CELLS: f32 = 2.0;
/// An inline image's width in cells (the prototype's `.img{width:48ch}`),
/// never wider than the image itself.
pub const IMAGE_CELLS: f32 = 48.0;

/// **The minimap** (the transcript's scrollbar): a `MINIMAP_W` rail at the
/// body's right edge, shown only while the pointer is on the transcript. A
/// `MINIMAP_TICK_H` tick per prompt (`ACCENT`), failed call (`BLOCKED`),
/// Decision (`ATTENTION`) and passing check (`RUNNING`), inset
/// `MINIMAP_TICK_INSET`; the viewport as a translucent band (`paint::HOVER`,
/// `paint::SELECTION` under the pointer) inset `MINIMAP_BAND_INSET`, never
/// shorter than `MINIMAP_BAND_MIN_H`. A click on a tick jumps to its row;
/// anywhere else centres the view on that point.
pub const MINIMAP_W: f32 = 12.0;
pub const MINIMAP_TICK_H: f32 = 2.0;
pub const MINIMAP_TICK_INSET: f32 = 3.0;
pub const MINIMAP_BAND_INSET: f32 = 1.0;
pub const MINIMAP_BAND_MIN_H: f32 = 12.0;
// (end WP-A) — append above this line only

// ======================================== WP-B · markdown, prose, scrollbars
// Owner: WP-B (rich.rs, scrollbar.rs, attachments::inline_file, the Markdown vendor knobs.)
// Edit values and append tokens only inside this section.

/// **Markdown** (the prototype's `.p`, `.code`, `table.t`, `ul.md`, clean).
/// Agent prose is the one face in `TEXT` at the reading size, left-aligned
/// on the content column and held to `MEASURE_CH` cells; code, tables and
/// rules keep the whole column. Blocks sit one blank line apart; headings
/// are the body size in `W_STRONG` `TEXT_STRONG`, `**strong**` the same.
/// Inline code is `INLINE_CODE` cyan with no chip; links and file paths are
/// `PATH_INK` cyan with no underline at rest (a file link underlines under
/// the pointer). A list's `•` is `TEXT_MUTED`, hanging in the first of two
/// cells. A fence has no ground: a 1px `paint::LINE2` rule on its left, the
/// code two cells in, in full syntax colour, its language tag dim at its
/// top right and `Copy` before it under the pointer. A table is a `W_STRONG`
/// `TEXT_STRONG` head over a `paint::LINE2` rule, row rules in `paint::LINE`,
/// no vertical rules and no box, figures tabular (a `---:` column aligns
/// right). A quote is a 2px `LINE2` rule and `TEXT_MUTED`; a thematic break
/// one `LINE`.
///
/// 12px — between Markdown blocks (`SPACE_3`), the transcript's block
/// step. This and the heading spaces are Standard values; other reading
/// sizes scale them with `reading_step`.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const PROSE_GAP: f32 = SPACE_3;
/// 8px — added above a heading that follows a sibling, on top of
/// `PROSE_GAP`, so a heading opens a section rather than closing one.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const HEADING_SPACE_ABOVE: f32 = SPACE_2;
/// 8px — below a heading, in place of `PROSE_GAP`.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const HEADING_SPACE_BELOW: f32 = SPACE_2;
/// Inline code's ink: cyan (`INLINE_CODE`, rule 6). The Markdown path
/// paints it; the plain-text fallback carries the ink alone.
pub const INLINE_CODE_INK: u32 = INLINE_CODE;
/// `#ffffff0f` (6%) — inline code's chip: a neutral ground that shows the
/// copy boundary (`None`, `nav.rs`) without tinting the line.
pub const INLINE_CODE_WASH: u32 = 0xffffff0f;
/// The inline-code chip reaches 2px past its glyphs. Painted, never laid
/// out; its height is `inline_code_chip_h`, centred in the prose line box.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const INLINE_CODE_OVERHANG: f32 = SPACE_0_5;

/// Inline code's size at each reading size: the UI size at 14 (`FS_UI`, so
/// a code cell is `CODE_CELL`), 1.5 under the prose up to 15 and 2 under it
/// above. Tables set their cells at the same size.
#[allow(dead_code)] // the previous grammar; remove after integration
pub fn inline_code_size(size: ferrite_core::settings::ReadingSize) -> f32 {
    let px = f32::from(size.px());
    if px <= 15. {
        px - (FS_PROSE - FS_UI)
    } else {
        px - 2.
    }
}

/// The inline-code chip's height: 4px over the prose size (18 at 14).
#[allow(dead_code)] // the previous grammar; remove after integration
pub fn inline_code_chip_h(size: ferrite_core::settings::ReadingSize) -> f32 {
    f32::from(size.px()) + 4.
}

/// How far the chip stays inside the prose line box, top and bottom:
/// `(answer_line_height − inline_code_chip_h) / 2`.
#[allow(dead_code)] // the previous grammar; remove after integration
pub fn inline_code_inset_y(size: ferrite_core::settings::ReadingSize) -> f32 {
    (answer_line_height(size) - inline_code_chip_h(size)) / 2.
}

/// A table row's line box: 6px over the prose size, so at 14 it is `LH_UI`
/// 20 and a row is 4 + 20 + 4 = 28, the list pitch.
#[allow(dead_code)] // the previous grammar; remove after integration
pub fn table_line_height(size: ferrite_core::settings::ReadingSize) -> f32 {
    f32::from(size.px()) + 6.
}

/// 570px — the prose measure (~88 characters of Geist at 14px): the most a
/// paragraph, a list item or a quote runs before it wraps. Fixed, not scaled
/// by the reading size. Code, tables, diffs, tool rows and the Composer keep
/// the whole `READING_MAX_W` column.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const PROSE_MEASURE: f32 = 570.0;
/// 28px — the one hang lists and quotes share at Standard: bullet and
/// ordered text start this far in, their markers right-aligned inside it
/// `LIST_MARKER_GAP` from the text (only a list whose ordinals reach 100
/// widens), and a quote's text lands on the same x past its rule. Each
/// nesting level adds another. Scales with the reading size (`reading_step`:
/// 28 · 32 · 36).
#[allow(dead_code)] // the previous grammar; remove after integration
pub const PROSE_HANG: f32 = 28.0;
/// 6px — between a list marker and its text.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const LIST_MARKER_GAP: f32 = SPACE_1_5;
/// A quote's rule. Its text inset is the hang less the rule
/// (`PROSE_HANG − QUOTE_RULE_W`, 26 at Standard), so quoted text starts
/// where list text does.
pub const QUOTE_RULE_W: f32 = 2.0;
/// 4px — a table cell's block padding, so a Standard row is 28px (its
/// inline padding is the vendor's 8px, which its column measurement
/// assumes).
#[allow(dead_code)] // the previous grammar; remove after integration
pub const TABLE_CELL_PAD_Y: f32 = SPACE_1;
/// The rule under a table's header row: one step stronger than the rows'.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const TABLE_HEAD_RULE: u32 = HAIRLINE_STRONG;
/// 4px — a horizontal rule's own margin inside its block, so it sits 16px
/// from its neighbours.
pub const RULE_MARGIN_Y: f32 = SPACE_1;
/// A fenced code block: 12px inline, 10px block padding (Zeron's code body;
/// 10 is off the scale so a one-line block is 10 + 18 + 10 = 38, and the
/// hover overlay's 24px actions centre on its first line).
#[allow(dead_code)] // the previous grammar; remove after integration
pub const CODE_PAD_X: f32 = SPACE_3;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const CODE_PAD_Y: f32 = 10.0;
/// A fence's actions overlay: the language id, html `Preview`,
/// `Copy`/`Copied`, top-right over the block. It is always laid out (so it
/// never moves the block) and only shown under the pointer (the 150ms
/// hover blend), while its keys have focus, or while the caret or a
/// selection is inside the block — those two instantly.
pub const CODE_ACTIONS_TOP: f32 = 0.0;
pub const CODE_ACTIONS_RIGHT: f32 = 0.0;
/// Code actions keep a stable target when Copy becomes Copied.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const CODE_ACTION_H: f32 = 24.;
pub const CODE_ACTION_MIN_W: f32 = 56.;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const CODE_ACTION_PAD_X: f32 = SPACE_2;
/// The html preview dialog: the reading column's width, and a height cap
/// before its body scrolls.
pub const HTML_PREVIEW_MAX_H: f32 = 520.0;
/// An inline file chip: `CHIP_H` tall so it fits a 22px prose line without
/// moving it; 6px inline padding; no file mark (an image leads with its
/// 14px thumbnail, 6px from the name); clamped between 64 and 280px wide.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const INLINE_FILE_H: f32 = CHIP_H;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const INLINE_FILE_PAD_X: f32 = SPACE_1_5;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const INLINE_FILE_GAP: f32 = SPACE_1_5;
#[allow(dead_code)] // the previous grammar; remove after integration
pub const INLINE_FILE_THUMB: f32 = 14.0;
pub const INLINE_FILE_MIN_W: f32 = CH;
pub const INLINE_FILE_MAX_W: f32 = 280.0;
/// **Scrollbars** are a thin overlay with no track, never in layout: a
/// `SCROLLBAR_GUTTER` hit strip at the scroller's right edge; while
/// scrolling a 4px `SCROLLBAR` thumb, under the pointer (or dragged) 6px
/// `SCROLLBAR_HOVER`; idle, nothing. The thumb keeps `SCROLLBAR_INSET` from
/// the edge and never gets shorter than `SCROLLBAR_MIN_THUMB`.
pub const SCROLLBAR_GUTTER: f32 = 12.0;
pub const SCROLLBAR_THUMB_W: f32 = 4.0;
pub const SCROLLBAR_THUMB_W_HOVER: f32 = 6.0;
pub const SCROLLBAR_INSET: f32 = 3.0;
pub const SCROLLBAR_MIN_THUMB: f32 = 40.0;
const _: () = assert!(SCROLLBAR_GUTTER == 2. * SCROLLBAR_INSET + SCROLLBAR_THUMB_W_HOVER);
const _: () = assert!(SCROLLBAR_GUTTER <= PANE_PAD_X);
// (end WP-B) — append above this line only

// ======================================== WP-C · pane frame, levels, board, titlebar
// Owner: WP-C (the pane shell, head, L2/wall cells, board, seams, titlebar.)
// Edit values and append tokens only inside this section.
//
// **The Pane frame.** A Pane is a `PANE` sheet with a 1px edge that is always
// in layout, so a state change recolours it and nothing reflows. The edge
// says one thing, by precedence (`pane::PaneEdge`): blocked `BLOCKED` >
// a Decision `ATTENTION` > focused `FOCUS_RING` > at rest `HAIRLINE`. On a
// board the resting hairline blends to `HAIRLINE_STRONG` under the pointer
// over the one 150ms hover blend; in Solo the frame never reacts to hover.
// Focus is drawn only while more than one Pane is on the board: a lone Pane
// is plainly the one with the keyboard and rests on its hairline. A
// *focused* alert Pane beside others also draws a `FOCUS_RING` ring inset
// by 2px, so focus is never hidden by a state. There is no other ring:
// unread breathes on the head dot (`ACCENT`, on the shared pulse clock at
// `MOTION_BREATH_MS`, held still under reduced motion).
//
// **Solo has no head** (C2): the titlebar carries the Thread —
// `project / ● title ⎇ branch · state` — and the body starts at the card
// edge. A strip of `PANE_HEAD_H` appears only while subagent tabs exist.
//
// **The Group head is one 32px line** (`PANE_HEAD_H`, rule 2.4.6) at every
// tier — L1, L2 and the wall — closed by a permanent `HAIRLINE` rule the
// body clips at: the status dot in the glyph box at `PANE_PAD_X` (every
// board's dots on one vertical), the title at C1 (`W_LABEL` `TEXT_STRONG`,
// flexing, never under `HEAD_TITLE_MIN_W`), the branch only when it is not
// the default, the provider mark only when it differs from the board's
// majority, then a fixed right slot with one lexicon word
// (`pane::HeadSlot`): `needs you · approval` > `failing 2` > `working 12s`
// > `done` > `ctx 84%` > a mode word. Nothing else rides the head.
//
// **One Level per board** (rule 2.3.5): the default Group tree is the
// aspect-aware grid (`layout::Tree::grid`), and every Pane on a board draws
// at the smallest Level its cells allow, with `LEVEL_HYSTERESIS`. Below L1
// the cells keep the L1 axes — marks at `PANE_PAD_X`, text at C1 — and a
// title is always `TEXT_STRONG`: the slot's word is the only signal.

/// The Windows caption buttons (`titlebar.rs`), which exist only where the
/// app draws its own titlebar. 46px is the width Windows gives each of its
/// own — the snap-layout flyout aligns to it, so a narrower button would
/// hang the flyout off-centre — and they run the band's full height,
/// flush to the window's top-right corner.
pub const CAPTION_W: f32 = 46.0;
/// 10px — the caption mark inside that button. Window chrome is smaller
/// than UI: `ICON_BUTTON_GLYPH` at 16px would read as an app control.
pub const CAPTION_GLYPH: f32 = 10.0;
/// 4px — the top edge a drag region leaves untagged, so the window can
/// still be resized from its top border. `SM_CYFRAME` is 4 logical pixels,
/// and gpui only reaches its own `HTTOP` fallback where no control area
/// answered first: a drag region flush to y = 0 would eat the resize edge
/// along the whole strip. A maximized window has no such edge and insets
/// nothing.
pub const CAPTION_RESIZE_EDGE: f32 = 4.0;
/// Platform chrome, not a Ferrite state colour: Windows' own close-button
/// field under the pointer and pressed, with its white mark. Muscle memory
/// wins over "colour is state" for this one control.
pub const CAPTION_CLOSE: u32 = 0xc42b1c;
pub const CAPTION_CLOSE_PRESSED: u32 = 0x9b2218;
pub const CAPTION_CLOSE_INK: u32 = 0xffffff;
/// The titlebar location's segments: 6px apart, one Geist baseline.
pub const TITLE_GAP: f32 = SPACE_1_5;
/// The Project's floor in a narrow titlebar: it truncates after the branch
/// but keeps a few letters, so the `/` never stands alone.
pub const TITLE_PROJECT_MIN_W: f32 = 48.0;
/// The titlebar's labelled add control: words on the ground (no box), a
/// 28px hit area with room for its label, the glyph 6px from it.
pub const TITLE_ADD_PAD_X: f32 = SPACE_2;
pub const TITLE_ADD_GAP: f32 = SPACE_1_5;
/// The smallest window the chrome still lays out in: the nav plus one Pane
/// at L2, the title, the add control and the Windows caption group.
pub const WINDOW_MIN_W: f32 = 640.0;
pub const WINDOW_MIN_H: f32 = 420.0;

/// How far every cell must clear a Level's size threshold, on both axes,
/// before the board steps *up* to it (`CockpitView::board_level`); it steps
/// down at the plain threshold. 24px keeps a resize that hovers at an edge
/// from flickering the whole board between tiers.
pub const LEVEL_HYSTERESIS: f32 = 24.0;

/// 32px — the Group head (and the Solo tab strip): one `LH_UI` line with 6px
/// of air, a 24px control (a draft's ×) fitting inside it.
pub const PANE_HEAD_H: f32 = 32.0;
/// The floor a head title keeps however narrow the head (a shorter title
/// keeps its whole text): the branch gives way first. There is no cap — a
/// long title takes the width the head has.
pub const HEAD_TITLE_MIN_W: f32 = 96.0;
/// Between the head's title, branch, provider mark and slot.
pub const HEAD_GAP: f32 = SPACE_2;
/// Between the tab strip's tabs and the plan's meter at its right.
pub const HEAD_CLUSTER_GAP: f32 = SPACE_3;
/// How much more the branch shrinks than the title when the head is
/// narrow: the branch gives way first.
pub const HEAD_CHECKOUT_SHRINK: f32 = 4.0;
/// The tasks meter in the head: 6 × 3 segments, 1px radius, 2px apart (an
/// 8px pitch). Past `METER_SEG_CAP` steps it is one `METER_TRACK_W` track.
pub const METER_SEG_W: f32 = 6.0;
pub const METER_SEG_H: f32 = 3.0;
pub const METER_SEG_GAP: f32 = SPACE_0_5;
pub const METER_SEG_R: f32 = 1.0;
pub const METER_SEG_CAP: usize = 12;
pub const METER_TRACK_W: f32 = 48.0;
/// Between the meter and its `3/4` count.
pub const METER_GAP: f32 = SPACE_1_5;
/// The checks card the head's PR/CI chip opens (#29): wide enough for a
/// matrix job's own name — `test (windows-latest, stable)` — beside its
/// state word, which is the whole reason the card exists.
pub const CHECKS_CARD_W: f32 = 312.0;
/// The card's widest: it grows to its tally and run names up to here.
pub const CHECKS_CARD_MAX_W: f32 = 420.0;
/// Between the card's heading and its runs: space, not a rule
/// (`MENU_GROUP_GAP`, as between any two groups on a floating surface).
pub const CHECKS_CARD_GAP: f32 = MENU_GROUP_GAP;
/// The card's heading row and one run's row: a menu row's height, so the
/// card lists at the same pitch as every other floating list.
pub const CHECKS_HEAD_H: f32 = MENU_ROW_H;
pub const CHECKS_ROW_H: f32 = MENU_ROW_H;
/// The space that sets a workflow's group off from the one before it. The
/// heading itself is the menu section title (`MENU_SECTION_H`).
pub const CHECKS_GROUP_GAP: f32 = MENU_GROUP_GAP;

/// The wall's signal line hangs 4px under the head rule, at the text
/// column (C1); its rows sit 4px apart. The dot is the head's own.
pub const WALL_ROW_GAP: f32 = SPACE_1;

/// A seam between Panes: the grab band is transparent, and a 2px line
/// inset 8px from each end (so it never touches a Pane corner) appears
/// `TEXT_FAINT` under the pointer and `ACCENT` while held.
pub const SEAM_LINE_W: f32 = 2.0;
pub const SEAM_LINE_INSET: f32 = SPACE_2;
/// A dragged Pane's drop wash: `DROP_WASH` ground, `ACCENT_EDGE` edge, the
/// Pane's radius; its label is a raised mono tag, 8/4 padded.
pub const DROP_LABEL_PAD_X: f32 = SPACE_2;
pub const DROP_LABEL_PAD_Y: f32 = SPACE_1;
/// The Pane a live drag picked up, dimmed in its slot until the release.
pub const DRAG_SOURCE_OPACITY: f32 = 0.5;
/// The miniature that rides the pointer while a slot is dragged: see-through
/// enough that the board under it still reads.
pub const DRAG_GHOST_OPACITY: f32 = 0.86;
/// The empty board's hints: lines 8px apart, the keys in one column 8px
/// from their verbs' shared edge.
pub const EMPTY_BOARD_GAP: f32 = SPACE_2;
/// 44px — the empty board's Ferrite mark, embossed on the field
/// (`components::embossed_mark`, rule 2.11.4) and centred
/// `EMPTY_BOARD_MARK_GAP` (24px) over the hints. It replaces the line of
/// words: the board says how to start, once.
pub const EMPTY_BOARD_MARK: f32 = 44.0;
pub const EMPTY_BOARD_MARK_GAP: f32 = SPACE_6;
/// An embossed mark's body on the field: two steps over `GROUND`, so the
/// mark reads as struck into the surface by its light and shadow edges,
/// not drawn on it in ink.
pub const EMBOSS_ON_GROUND: u32 = 0x1d2024;
/// The same on a Pane's ground (an empty draft's body).
pub const EMBOSS_ON_PANE: u32 = 0x23262b;
/// The emboss's lit upper edge and its shadowed lower edge (legacy: the
/// foundation draws the mark flat, theme rule 2).
#[allow(dead_code)]
pub const EMBOSS_LIGHT: u32 = 0xffffff17;
#[allow(dead_code)]
pub const EMBOSS_SHADE: u32 = 0x000000b3;
/// 44px — an empty draft's mark, embossed over its Composer (Solo) or at
/// its body's centre (a board): the Pane is ready, and the Composer's
/// placeholder still says what to do, once.
pub const DRAFT_MARK: f32 = 44.0;
/// 24px — from a launching draft's mark down to its Composer.
pub const DRAFT_MARK_GAP: f32 = SPACE_6;
/// 64px — how far a launching draft's pair sits above the Pane's middle:
/// the optical centre is above the measured one.
pub const DRAFT_LAUNCH_LIFT: f32 = 64.0;
// (end WP-C) — append above this line only

// ======================================== WP-D · composer, pickers, usage, draft
// Owner: WP-D (the Composer, its pickers and usage meter, attachments, background chips, the draft.)
// Edit values and append tokens only inside this section.

/// **The Composer is a raised block in the reading column.** Its outer edges
/// are the column's edges (`reading_column`), its content inset is
/// `BOX_INSET_X` (1px edge + `COMPOSER_PAD_X`), so its `❯` hangs in the same
/// glyph box as the transcript's and its text starts at the same C1. It is
/// `RAISED` with a 1px `COMPOSER_EDGE` that is always in layout; the edge
/// turns `ACCENT_EDGE` only while native files hover it. State never
/// recolours it: the Pane ring, the accent `❯` and the caret carry focus.
/// Typed input is mono `TEXT` at 400; a selected run takes `TEXT_STRONG` on
/// `COMPOSER_SELECTION`. Rows are `COMPOSER_ROW_H`, `COMPOSER_GAP`
/// apart: queued prompts (dim `❯` lines), then the one input row — `❯` and
/// the line at left, the model pair and the round send control at right.
/// Under the box, outside it, the status line (`COMPOSER_META_H`, one
/// `LH_META` line, `COMPOSER_META_GAP` below the box), present in every
/// Solo state and reserved when empty: the mode word (`mode_word`, hidden
/// at the default) and a draft's setup chips at left, session controls and
/// `ctx 32%` as text at right, `FS_SM` `TEXT_MUTED`. The meta row's ink shares the box's text edges: its first
/// label starts at C1 (`COMPOSER_META_START`), its last mark ends on the
/// send control's trailing edge (`COMPOSER_META_END`); the chips' own
/// padding hangs outside those edges. The placeholder is a ladder of
/// rungs (`Steer this thread… · / for commands`, `Steer this thread…`,
/// `Steer…`): the line shows the longest that fits and never cuts a word;
/// its one key hint follows a `TEXT_FAINT` `·`. The controls' tooltips name
/// their keys (`Send ↵`, `Interrupt esc`). Any pad, gap, edge or inset
/// change here must update `pane::composer_fixed_height` in the same commit.
pub const COMPOSER_PAD_X: f32 = BOX_INSET_X - 1.0;
pub const COMPOSER_PAD_T: f32 = SPACE_2;
pub const COMPOSER_PAD_B: f32 = SPACE_2;
/// The trailing padding equals the vertical one, so the send control sits
/// `COMPOSER_CONTROL_INSET` from the box's top, bottom and trailing edges —
/// the even inset concentric corners need. The leading side keeps
/// `COMPOSER_PAD_X`, which puts the `❯` on the transcript's glyph axis.
pub const COMPOSER_PAD_END: f32 = COMPOSER_PAD_T;
pub const COMPOSER_ROW_H: f32 = 20.0;
pub const COMPOSER_GAP: f32 = SPACE_1;
pub const COMPOSER_META_H: f32 = LH_META;
pub const COMPOSER_META_GAP: f32 = SPACE_1;
/// The send control: a `COMPOSER_ROW_H` square on `R_CHIP` corners, its
/// glyph 10px. At rest it
/// sends (↑); while a turn runs it stops (■), whatever is in the line —
/// Enter is the key that queues a line behind the turn.
pub const SEND_BUTTON: f32 = COMPOSER_ROW_H;
pub const SEND_GLYPH: f32 = 10.0;
/// **Only an armed Send is bright** (C27, rule 2.2.7). A draft that can go
/// makes it the one bright square in the Pane: `TEXT_STRONG` with the glyph
/// in the Pane's ground, stepping down to `TEXT` under the pointer and
/// `TEXT_2` pressed. It switches on, unblended, on the keystroke that makes
/// the line sendable. Idle (an empty line at rest) it keeps its shape,
/// legible but plainly off: a `FILL_HOVER` square with a `TEXT_MUTED` glyph.
/// Stop is always present while a turn runs, so it never takes the bright
/// ground: `SEND_STOP_GROUND` (`FILL`) with a `TEXT_2` `■`, stepping to
/// `FILL_HOVER` and a `TEXT_STRONG` glyph through the pointer blend
/// (150ms); press is instant.
pub const SEND_GROUND: u32 = TEXT_STRONG;
pub const SEND_INK: u32 = PANE;
pub const SEND_HOVER: u32 = TEXT;
pub const SEND_PRESSED: u32 = TEXT_2;
pub const SEND_IDLE_GROUND: u32 = FILL_HOVER;
pub const SEND_IDLE_INK: u32 = TEXT_MUTED;
pub const SEND_STOP_GROUND: u32 = FILL;
// Rule 2.2.7 / C27: an always-present control is never the brightest ground.
const _: () = assert!(SEND_STOP_GROUND != TEXT_STRONG);
pub const SEND_STOP_HOVER: u32 = FILL_HOVER;
pub const SEND_STOP_INK: u32 = TEXT_2;
pub const SEND_STOP_INK_HOVER: u32 = TEXT_STRONG;
/// The block's 1px edge, top and bottom: part of its fixed height.
pub const COMPOSER_EDGE_W: f32 = 1.0;
/// **A block, not a pill.** The box is a terminal line on `R_BLOCK`, like
/// every raised block, and its controls (chips, the send square) sit on
/// `R_CHIP`. The inset between them (`COMPOSER_CONTROL_INSET`: the edge and
/// the vertical padding) is wider than the box's corner, so by the radius
/// rule the controls take their own role radius. A setup chip's focus is
/// the one inset ring (`components::control_focus`), inside its corner. The
/// Subagent footer, the Composer's own block, shares the corner.
pub const COMPOSER_CHIP_R: f32 = R_CHIP;
pub const COMPOSER_CONTROL_INSET: f32 = COMPOSER_EDGE_W + COMPOSER_PAD_T;
pub const COMPOSER_R: f32 = R_BLOCK;
/// Where the meta row's ink starts and ends, as padding on the row: C1 and
/// the send control's trailing edge, less the `PICKER_PAD_X` each chip
/// hangs outside its label.
pub const COMPOSER_META_START: f32 = BOX_INSET_X + GUTTER_W - PICKER_PAD_X;
pub const COMPOSER_META_END: f32 = COMPOSER_CONTROL_INSET - PICKER_PAD_X;
/// 8px — from the block to the Pane's bottom edge at L1 (the transcript's
/// own bottom padding supplies the air above it), and the L2 cell's inset
/// around its compact Composer on three sides. The block's 6px vertical
/// padding keeps a one-line Composer at 58px + this inset.
pub const COMPOSER_INSET_B: f32 = SPACE_2;
pub const COMPOSER_INSET_L2: f32 = SPACE_2;
/// 7px — the L2 box's inline padding: `PANE_PAD_X` less the inset and the
/// edge (16 − 8 − 1), so the compact Composer's `❯` sits on the glyph
/// column at x = 16, over the tail's marks.
pub const COMPOSER_PAD_X_L2: f32 = PANE_PAD_X - COMPOSER_INSET_L2 - COMPOSER_EDGE_W;
/// **The grid Composer line** (C4): every board cell's Composer is one
/// fixed 32px line — `COMPOSER_GRID_PAD_Y` above and below one
/// `COMPOSER_ROW_H` row, inside the 1px edge — with no status row, so
/// cmd-] across a board moves no tail. Only the focused cell's line is
/// raised, edged and carries a caret; the others lie flat.
pub const COMPOSER_GRID_PAD_Y: f32 = 5.0;
pub const COMPOSER_GRID_H: f32 = 2.0 * COMPOSER_EDGE_W + 2.0 * COMPOSER_GRID_PAD_Y + COMPOSER_ROW_H;
/// Multiline drafts, controls and queued prompts share a bounded part of
/// the Pane, keeping most of its height available to the conversation.
pub const COMPOSER_MAX_PANE_FRACTION: f32 = 0.45;
/// The queued-prompt viewport scrolls beyond these visible row budgets.
pub const COMPOSER_QUEUE_ROWS: usize = 3;
pub const COMPOSER_COMPACT_QUEUE_ROWS: usize = 1;
/// 8px — between the shelf (pending files, background chips) and the
/// block. The shelf's first chip sits on the block's outer left edge.
pub const SHELF_GAP: f32 = SPACE_2;
/// The caret: 2 × 16 in `CARET` (the accent), square, centred on integer
/// pixels in the 20px row (16 covers Geist Mono's ascender and descender at
/// `FS_UI`).
pub const CARET_W: f32 = 2.0;
pub const CARET_H: f32 = 16.0;
/// One selection colour app-wide: the Composer paints the transcript's
/// native selection wash under its selected runs.
pub const COMPOSER_SELECTION: u32 = TEXT_SELECTION_WASH;
/// An `@`-mention the operator picked: `TEXT` on the neutral inline-code
/// wash — the accent is only for the prompt mark, caret, links, focus,
/// selection and the primary button (rule 2.2.6).
pub const MENTION_INK: u32 = TEXT;
pub const MENTION_WASH: u32 = INLINE_CODE_WASH;
/// **Composer controls are quiet chips** (model, effort, mode, session
/// `•••`, the `ctx` readout): `CHIP_H`, `PICKER_PAD_X` both sides,
/// `COMPOSER_CHIP_R`, no ground at rest, `FILL` under the pointer (the hover face on `RAISED`),
/// label `FS_SM` `TEXT_2`, a `ICON_CHEVRON_SM` chevron in `TEXT_MUTED`. A
/// busy control reads `TEXT_MUTED`, never faded. The model and effort pair
/// sits `PICKER_GAP` apart and reads as one unit.
pub const PICKER_PAD_X: f32 = SPACE_1_5;
pub const PICKER_GAP: f32 = SPACE_1;
pub const ICON_CHEVRON_SM: f32 = 10.0;
/// The usage meter's detail card: one column of labelled bars, sized so
/// the three windows read at a glance without the card becoming a panel.
/// Each block insets its text by `MENU_ROW_PAD_X`, so it sits on a menu
/// row's edge inside the floating surface's `FLOAT_PAD`.
pub const USAGE_CARD_W: f32 = 288.0;
/// Between one window's block and the next, and inside one block: the
/// blocks stand twice as far apart as their own lines.
pub const USAGE_CARD_GAP: f32 = SPACE_3;
pub const USAGE_CARD_ROW_GAP: f32 = SPACE_1_5;
pub const USAGE_CARD_BAR_H: f32 = 4.0;
/// Where a usage reading turns from neutral to ATTENTION (80%, rule
/// 2.6.6) — a fraction of the window, not a count — on the status line and
/// the card alike. There is no BLOCKED step: a full window stops nothing
/// until the provider says so. Below tight the status line's `ctx 32%` is
/// `TEXT_MUTED`: colour is state, and a context half full is not a state.
pub const USAGE_TIGHT: f32 = 0.80;
/// The session-controls card: permission modes, MCP servers and background
/// tasks as sections of menu rows, wide enough for a server's name beside
/// its state and two quiet actions.
pub const SESSION_CARD_W: f32 = 320.0;
/// The changed-files card: wide enough for a file name, a short directory
/// and its `+N −N` on one menu row.
pub const CHANGED_FILES_CARD_W: f32 = 360.0;
/// One row of the usage card's legend, its colour square, and the share
/// column the percentages right-align in.
pub const USAGE_LEGEND_ROW_H: f32 = 20.0;
pub const USAGE_SWATCH: f32 = 8.0;
pub const USAGE_SHARE_W: f32 = 44.0;
/// What fills the context window, one ink per category in the stacked bar
/// and its legend. The inks are the palette's own quiet hues, not new ones:
/// the breakdown has to tell categories apart, not shout. What the operator
/// wrote leads (accent), the machine's work follows in the muted state hues,
/// and the fixed overheads fade through grey to the free space. Anything
/// else a provider reports takes `CTX_CYCLE`.
pub const CTX_MESSAGES: u32 = ACCENT;
pub const CTX_TOOLS: u32 = 0xc4a58e;
pub const CTX_MCP: u32 = RUNNING;
pub const CTX_SKILLS: u32 = ATTENTION;
pub const CTX_PROMPT: u32 = 0x9aa0a8;
pub const CTX_MEMORY: u32 = 0x7c828b;
pub const CTX_BUFFER: u32 = 0x5c6168;
pub const CTX_FREE: u32 = 0x33373d;
pub const CTX_DEFERRED: u32 = 0x464a51;
pub const CTX_CYCLE: [u32; 4] = [0xa79fc4, 0xc39cab, 0x8fb2b8, 0xb5b48e];
/// 768px — the image preview sheet's widest reading; it otherwise takes
/// 90% × 85% of its Pane.
pub const PREVIEW_MAX_W: f32 = 768.0;
/// Background task chips on the shelf above the Composer: `FILL` (not
/// `HOVER`, which vanishes on `RAISED`), `R_CHIP`, `CHIP_H`, mono `FS_SM`
/// `TEXT_2`, the shared pulsing `RUNNING` dot, labels cut at 240px, and a
/// quiet `×` stop control (`BG_CHIP_STOP` square, glyph `TEXT_MUTED`).
pub const BG_CHIP_MAX_W: f32 = 240.0;
pub const BG_CHIP_STOP: f32 = 16.0;
pub const BG_CHIP_STOP_GLYPH: f32 = 10.0;
/// A pending file on the shelf: a `CHIP_H` chip with a fixed 12px slot for
/// its thumbnail (`R_TIGHT` corners) or the `FILE` mark, the name mono
/// `FS_SM` `TEXT_2` cut at 200px.
pub const ATTACH_CHIP_H: f32 = CHIP_H;
pub const ATTACH_CHIP_MAX_W: f32 = 200.0;
pub const ATTACH_THUMB: f32 = 12.0;
// (end WP-D) — append above this line only

// ======================================== WP-E · menus, popovers, sheets, notifications
// Owner: WP-E (menus, popovers, Settings, the Project editor, notifications.)
// Edit values and append tokens only inside this section.

/// Form fields and segmented choices share a 32px row. Compact pane and
/// navigation controls keep their own smaller chrome metrics.
pub const FORM_CONTROL_H: f32 = 32.0;
/// Inset around the chips of a segmented choice control, and the gap
/// between them: 2px, so the chips nest concentrically (`R_CHIP` inside the
/// tray's `R_CONTROL`) and the tray reads as one control.
pub const FORM_CHOICE_PAD: f32 = SPACE_0_5;
/// A choice chip's and a chooser's inline padding inside the 32px row.
pub const FORM_CHIP_PAD_X: f32 = SPACE_2;
pub const FORM_FIELD_PAD_X: f32 = SPACE_2 + SPACE_0_5;
/// The Settings switch: a 32×18 pill (2px inset), a 14px thumb travelling
/// the pill's inner width — a macOS-sized switch in a 36px setting row.
pub const SWITCH_W: f32 = 32.0;
pub const SWITCH_H: f32 = 18.0;
pub const SWITCH_INSET: f32 = SPACE_0_5;
pub const SWITCH_THUMB: f32 = SWITCH_H - 2.0 * SWITCH_INSET;
pub const SWITCH_TRAVEL: f32 = SWITCH_W - 2.0 * SWITCH_INSET - SWITCH_THUMB;
/// A sheet text button's inline padding (Add Directory, Remove, Done).
pub const FORM_BUTTON_PAD_X: f32 = SPACE_3;
/// A tooltip: UI `FS_SM`, 8px × 4px, `R_CONTROL`, at most 280px before it
/// wraps. It is how a truncated label keeps its full value reachable.
pub const TOOLTIP_PAD_X: f32 = SPACE_2;
pub const TOOLTIP_PAD_Y: f32 = SPACE_1;
pub const TOOLTIP_MAX_W: f32 = 280.0;
/// The notifications panel: 340px holds a title, a detail line and an age
/// without wrapping; a row is two lines in 6px of air each side. No rules
/// and no head row inside it: each section (`Needs you N`, `Earlier`) is a
/// `MENU_ROW_H` label sitting directly on its rows, and the sections stand
/// `GAP_BLOCK` apart.
pub const NOTICE_PANEL_W: f32 = 340.0;
pub const NOTICE_ROW_H: f32 = LH_UI + LH_META + 2.0 * SPACE_1_5;
/// 30px — a notification row's age slot: `12mo` at tabular `FS_SM`, kept
/// even while a fresh request's age says nothing (`facts::since_label`).
pub const NOTICE_AGE_W: f32 = 30.0;
/// The bell's unread count (rule 2.2.9, colour on the word, never the
/// ground): one `FS_SM` line high and at least as wide, tabular `W_BODY`
/// figures on `FILL_HOVER`, flush with the button's top and starting 2px
/// right of its centre, so the glyph stays readable beside it.
pub const BADGE_H: f32 = LH_META;
pub const BADGE_LEFT: f32 = ICON_BUTTON / 2.0 + SPACE_0_5;
/// 2px — the badge's knockout: a ring of the nav's ground cut around the
/// pill, so the bell's stroke stops short of the count instead of running
/// under it.
pub const BADGE_KNOCKOUT: f32 = SPACE_0_5;
/// With the nav collapsed, toasts stack BottomRight this far up: the
/// board's padding, the Pane's edge, a one-line Composer and its inset,
/// then 8px of air, so the stack clears the Composer.
pub const TOAST_ABOVE_COMPOSER: f32 = GRID_PAD
    + 1.0
    + COMPOSER_INSET_B
    + 2.0 * COMPOSER_EDGE_W
    + COMPOSER_PAD_T
    + COMPOSER_PAD_B
    + COMPOSER_ROW_H
    + COMPOSER_META_GAP
    + COMPOSER_META_H
    + SPACE_2;
/// Settings and Project editors share the same header and content insets.
pub const MODAL_HEAD_H: f32 = 48.0;
pub const MODAL_PAD: f32 = 16.0;
pub const MODAL_GAP: f32 = 12.0;
/// Editors leave an even breathing edge while making room for a scrolling
/// form at short desktop heights.
pub const MODAL_VIEWPORT_FRACTION: f32 = 0.92;
/// A kit-hosted choice menu (model, effort, mode, subagent overflow): wide
/// enough for a model name beside its check, capped before it crowds the
/// Composer it opens from.
pub const CHOICE_MENU_MIN_W: f32 = 240.0;
pub const CHOICE_MENU_MAX_W: f32 = 320.0;
/// About 48 characters — how much of a long directory a menu row keeps,
/// cut at its head behind `…/` (the tail names the place).
pub const MENU_PATH_TAIL: usize = 48;
// ---------------------------------------------------------- Settings sheet
//
// **The Settings sheet** (`prefs.rs`) is Ferrite's own, in the manner of
// macOS System Settings, Zed and Linear, sized to its content
// (`SETTINGS_W` × `SETTINGS_H`, capped to `MODAL_VIEWPORT_FRACTION` of the
// window). The sheet itself is `PANE`, one step under the cards it holds,
// with the sheet recipe's strong edge, `R_PANE` and float shadow over the
// veil. Its head is the title and the one close control, set apart by
// space alone: no rule.
//
// - **Sidebar** (`SETTINGS_SIDEBAR_W`): the search field on top, then one
//   plain row per page — a 16px line mark, then the label — at the one
//   list pitch (`SETTINGS_NAV_ROW_H` = `NAV_ROW_H`, 28). No chevrons: a
//   row is a place, not a disclosure. The selected row is a `FILL` ground
//   with `TEXT_STRONG`; the rest `TEXT_2` with their marks in `TEXT_MUTED`,
//   hovering to `HOVER`. `About` is pinned to the sidebar's foot, apart
//   from the settings. ↑/↓ step pages while the sheet holds focus.
// - **Content:** the page title (`FS_PROSE` `W_LABEL` `TEXT_STRONG`), then
//   groups `SETTINGS_GROUP_GAP` apart. A group is a quiet sentence-case
//   section label (`FS_SM` `W_LABEL` `TEXT_MUTED`, on the rows' text edge,
//   a provider group led by its logomark in brand colour) above one card:
//   `RAISED`, `R_BLOCK`, a `HAIRLINE` edge. Rows inside the card sit flush,
//   split by `HAIRLINE`s, so the gap between groups is far more than twice
//   the gap between rows.
// - **Row:** the label (`FS_UI` `TEXT`) over an optional one-line hint
//   (`FS_SM` `TEXT_MUTED`, under 60 characters, saying what the setting
//   does) at the left; the control right-aligned and vertically centred.
//   `SETTINGS_ROW_H` (36) without a hint, `SETTINGS_ROW_HINT_H` (44) with.
// - **Controls** are `SETTINGS_CONTROL_H` (28) and hug their content: a
//   switch for on/off; a menu button (the value, then a chevron, on
//   `RAISED_2`, never a full-width field, at most `SETTINGS_MENU_MAX_W`)
//   for a choice from a list; a segmented tray for two options. Each has
//   the one hover blend, an instant press and the focus outline. Inside a
//   card the inset exceeds every control's radius, so each keeps its own
//   role radius (`R_CONTROL`, and `R_CHIP` for a tray's chips, concentric
//   with the tray).
// - **About** is one card of facts: the key at the left, the value at the
//   right in Geist Mono `TEXT_2` (versions and paths are machine text).
//   A path copies on click; its row shows the copy mark on hover and a
//   check once copied.
// - **Search** filters every page at once: matching rows keep their cards,
//   each labelled `Page · Group`; pages with no match fade to
//   `TEXT_MUTED` in the sidebar, and choosing a page clears the search.

/// The sheet: wide enough for a sidebar and a 500px card column, tall
/// enough for the longest page (Behaviour: a card of one row and a card
/// of four) without a scroll, and no taller. Search results may scroll.
pub const SETTINGS_W: f32 = 760.0;
pub const SETTINGS_H: f32 = 448.0;
/// The sidebar column, the search field included.
pub const SETTINGS_SIDEBAR_W: f32 = 196.0;
/// A sidebar row and the search field: the one list pitch.
pub const SETTINGS_NAV_ROW_H: f32 = NAV_ROW_H;
/// The search field's magnifier: a step under a row's mark, as the
/// field's placeholder is a step under a label.
pub const SETTINGS_SEARCH_ICON: f32 = 14.0;
/// A sidebar row's mark and the gap to its label.
pub const SETTINGS_NAV_ICON: f32 = ICON_BUTTON_GLYPH;
pub const SETTINGS_NAV_ICON_GAP: f32 = SPACE_2;
/// A setting row: one `LH_UI` line in 8px of air each side, or the line
/// and its `LH_META` hint in 4px.
pub const SETTINGS_ROW_H: f32 = LH_UI + 2.0 * SPACE_2;
pub const SETTINGS_ROW_HINT_H: f32 = LH_UI + LH_META + 2.0 * SPACE_1;
/// A row's inline padding inside its card; group labels sit on the same
/// edge.
pub const SETTINGS_ROW_PAD_X: f32 = SPACE_3;
/// Between groups, and between a group's label and its card.
pub const SETTINGS_GROUP_GAP: f32 = SPACE_6;
pub const SETTINGS_LABEL_GAP: f32 = SPACE_2;
/// Between the page title and its first group.
pub const SETTINGS_TITLE_GAP: f32 = SPACE_4;
/// Every control in a row: the chrome control height.
pub const SETTINGS_CONTROL_H: f32 = CONTROL_H;
/// The text-size stepper's value slot: `24px` never moves the `+`.
pub const SETTINGS_STEPPER_VALUE_W: f32 = 44.0;
/// A menu button's widest value before it truncates (a model's name).
pub const SETTINGS_MENU_MAX_W: f32 = 220.0;
/// An About key's column, so every value starts on one edge.
pub const SETTINGS_FACT_KEY_W: f32 = 104.0;
// (end WP-E) — append above this line only

// ======================================== WP-F · decisions and subagents
// Owner: WP-F (decision.rs, subagents.rs, the Decision card and keycaps.)
// Edit values and append tokens only inside this section.

/// The Decision block (approvals, questions, forms, links — Main's and a
/// Subagent's alike, rule 2.8), in the provider CLIs' own grammar (the
/// prototype's `.dec`, clean): no box, a 1px `paint::LINE` above it where
/// it docks over the transcript, the plane under it. `◆ Bash needs approval
/// · codex` heads it — the drawn `◆` (`DECISION_MARK`) in the 2-cell
/// gutter, the lead in `ATTENTION`, each detail after a `TEXT_FAINT` `·` in
/// `TEXT_MUTED` — and every other section sits on the content column after
/// the gutter, `DECISION_GAP` (half a row) apart: the prose, the command on
/// a `paint::BAND` behind a faint `$ `, the options as one-row lines
/// (`❯ 1. Allow`: the accent `❯` on the row ↵ would choose, the ordinal
/// dim, a picked row on `paint::SELECTION`, the pointer's on
/// `paint::HOVER`), and a dim hint line (`1–3 pick · y allow · n deny`).
/// No keycaps, no pills. Deny is not red. Every section keeps its natural
/// height — nothing clips; when the Pane runs short the prose goes first,
/// then the command band gives way down to one line.
#[allow(dead_code)] // the previous grammar; remove after integration
pub const DECISION_PAD_X: f32 = CH;
pub const DECISION_PAD_Y: f32 = HALF_ROW;
pub const DECISION_GAP: f32 = HALF_ROW;
/// The head's drawn diamond: 10px in the gutter of an `LH_UI` line.
pub const DECISION_MARK: f32 = 10.0;
/// An option row (rule 2.8.4): one `LH_UI` row, flush with the next; a
/// description adds a line under the label. Its `❯` gutter is two cells.
pub const DECISION_ROW_PAD_X: f32 = 0.0;
pub const DECISION_ROW_PAD_Y: f32 = 0.0;
pub const DECISION_ROW_GAP: f32 = 0.0;
pub const DECISION_ROW_INNER_GAP: f32 = 0.0;
/// The picked row's trailing check.
pub const DECISION_CHECK: f32 = SPACE_3;
/// A question's text to its rows, and one question to the next.
pub const DECISION_QUESTION_GAP: f32 = HALF_ROW;
pub const DECISION_QUESTIONS_GAP: f32 = ROW;
/// The command band (C5): one cell inside, a quarter row above and below,
/// mono `TEXT_STRONG`, a shell command after a `TEXT_FAINT` `$ ` that copy
/// leaves out. It scrolls past 160px, and when the Pane is short it is the
/// one section that shrinks, never below one line (`DECISION_WELL_MIN_H`).
pub const DECISION_WELL_PAD_X: f32 = CH;
pub const DECISION_WELL_PAD_Y: f32 = ROW / 4.0;
pub const DECISION_WELL_MIN_H: f32 = LH_UI + 2.0 * DECISION_WELL_PAD_Y;
pub const DECISION_INPUT_MAX_H: f32 = 160.0;
/// A question body's scroll cap inside the card (head and footer stay
/// pinned); container-relative, never a window fraction.
pub const DECISION_BODY_MAX_H: f32 = 320.0;
/// Below a 360px Pane the body caps at two described option rows and
/// scrolls, so the head and the answer row always stay in reach.
pub const DECISION_SHORT_PANE_H: f32 = 360.0;
pub const DECISION_SHORT_BODY_MAX_H: f32 = 2.0 * (LH_UI + LH_UI) + DECISION_ROW_GAP;
/// The scroll gutter a body keeps free for its thumb.
pub const DECISION_SCROLL_GUTTER: f32 = SPACE_1;
/// A question's own answer (rule 2.8.6) is not a second field: it is one
/// bare mono input line on the text column, `LH_UI` high, placeholder
/// `Or type your own answer…` in `TEXT_MUTED`; the digit one past the last
/// option arms it.
pub const QUESTION_OTHER_H: f32 = LH_UI;
/// The primary's `↵`: `ON_ACCENT` at 70%, so the key reads under its label
/// on the accent fill.
pub const SEND_KEY_INK: u32 = 0x111214b3;
/// An L2 quick-answer pair (`y allow`); pairs one cell apart.
pub const DECISION_KEY_GAP: f32 = SPACE_1;
pub const DECISION_KEYS_GAP: f32 = CH;
/// The L2 Decision body: 6px between its lines, `GAP_BLOCK` above the
/// Composer line.
pub const DECISION_L2_GAP: f32 = SPACE_1_5;

/// Subagent tabs (rule 2.2.4-5): their own `SUBJECT_STRIP_H` row at the
/// Pane's top — under the Group head in a board cell — closed by a
/// permanent `HAIRLINE` the body clips at. 20px tabs packed with no gap
/// and centred in the row, 8px inline padding, no edge; the active tab a
/// `FILL` pill in `TEXT_STRONG` (`FILL_HOVER` under the pointer), the
/// others `TEXT_MUTED` blending to `TEXT` with no ground. Main's label sits
/// on the text column. Labels truncate at 112px. A still `STATUS_DOT`
/// leads a subagent's label 6px before it, in a slot every tab reserves —
/// waiting `ATTENTION` > failed `BLOCKED` > working `RUNNING` — so a tab
/// never changes width with its state; the `+N` overflow uses the same
/// slot.
pub const SUBJECT_TAB_PAD_X: f32 = SPACE_2;
pub const SUBJECT_TAB_GAP: f32 = SPACE_1;
pub const SUBJECT_TAB_INNER_GAP: f32 = SPACE_1_5;
pub const SUBJECT_LABEL_MAX_W: f32 = 112.0;
pub const SUBJECT_STRIP_H: f32 = PANE_HEAD_H;
// (end WP-F) — append above this line only

// ======================================== WP-G · nav
// Owner: WP-G (nav.rs and its cockpit wiring.)
// Edit values and append tokens only inside this section.
//
// **The nav is one column grid on `GROUND`, in Geist.** Every row — a
// Thread, a Group, a Project heading, the Parked header, and the filter
// trigger in the head — lays out `ROW_PAD_X | lead slot NAV_LEAD_W |
// NAV_LEAD_GAP | text … | tail | mark`. The 16px lead slot holds the row's
// one glyph (status dot, Group glyph, folder, fold chevron), so every title
// and label starts on one x (`NAV_TEXT_X` 22), and the head's folder sits
// on the same axis as the rows' dots (head inset 8 + trigger inset 8 =
// tree inset 8 + row inset 8).
//
// **One 28px line per row** (`NAV_ROW_H`, C9): the dot, the title at
// `FS_UI` `W_BODY` (`TEXT`; `TEXT_STRONG` when selected or unread, ink
// only), the branch inline in `TEXT_MUTED` only when it is not the
// Project's default, then the tail, then the provider mark in its brand
// colour. There is no `project · branch` line.
//
// **The title comes first** (`nav::title_fit`). It keeps a floor of
// `min(its whole text, NAV_TITLE_FLOOR)`; the branch gives way first — it
// truncates, and below `NAV_BRANCH_MIN_W` it leaves the row whole — then
// the subagent count drops out whole; the tail's word or age and the
// provider mark never give way. The same order holds in a Group's member
// rows, the Needs-you strip and the rail's tooltip; the row's tooltip
// always names the whole title and the count.
//
// **The tail is one word or an age** (C10, `nav::NavTail`): `needs you`
// (`ATTENTION`), `failing N`/`failed` (`BLOCKED`), `done` (`TEXT_MUTED`,
// unread only), otherwise the age once it reaches a minute (`FS_SM`
// `TEXT_MUTED`, tabular). A working row says nothing there; the tail never
// reads `now`. Its box keeps `NAV_TAIL_MIN_W`, so a word arriving moves
// nothing.
//
// **Dots are still; only unread breathes** (C11). Working is a static
// `RUNNING` dot, failing a static `BLOCKED` one, a Decision a static
// `ATTENTION` one, parked a hollow ring. Unread is an `ACCENT` dot whose
// opacity alone breathes on the shared clock at `MOTION_BREATH_MS`, held
// at full ink under reduced motion. Selection is one `FILL` on the focused
// Thread's row, with no ring; nothing else fills.
//
// **Needs you** (C8): while any Thread waits, a strip under the head lists
// every one in answer order — its first row is what ⌘D and the wall's
// `y`/`n`/`a` act on — and the tree below never re-sorts under the pointer.

/// 42px — the nav head band, which holds the Project filter.
pub const NAV_HEAD_H: f32 = 42.0;
/// 6px between the head's controls.
pub const NAV_HEAD_GAP: f32 = SPACE_1_5;
/// The nav tree's padding: 8px top and inline, 16px bottom.
pub const NAV_TREE_PAD: f32 = SPACE_2;
pub const NAV_TREE_PAD_B: f32 = SPACE_4;
/// 28px — the Project filter trigger.
pub const FILTER_TRIGGER_H: f32 = ICON_BUTTON;
/// Where the filter and order menus hang: under the trigger, which is
/// centred in the head, plus the float offset every popup keeps from its
/// opener.
pub const MENU_TOP: f32 = (NAV_HEAD_H + FILTER_TRIGGER_H) / 2.0 + FLOAT_OFFSET;
/// 224px — the order menu, anchored to its button at the head's right.
pub const NAV_ORDER_MENU_W: f32 = 224.0;
/// 16px — a row's lead slot: the status dot, the Group glyph, the folder in
/// the filter trigger and a Project heading, the Parked chevron.
pub const NAV_LEAD_W: f32 = SPACE_4;
/// 6px — from the lead slot to the row's text.
pub const NAV_LEAD_GAP: f32 = SPACE_1_5;
/// 22px — where a row's text starts inside its own padding: the title, a
/// heading's label.
pub const NAV_TEXT_X: f32 = NAV_LEAD_W + NAV_LEAD_GAP;
/// 8px — from the tail to the provider mark at the row's right.
pub const NAV_MARK_GAP: f32 = SPACE_2;
/// 4px — between a title and its inline branch, between the subagent count
/// and the tail, and between a fact and its `·` seam.
pub const NAV_TAIL_GAP: f32 = SPACE_1;
/// 120px — the floor a nav title keeps before the subagent count gives way
/// (about nineteen characters at `FS_UI`): off the space scale because it
/// is a reading measure, not a gap. A shorter title keeps its own width.
pub const NAV_TITLE_FLOOR: f32 = 120.0;
/// 40px — the narrowest an inline branch is drawn (`·` and a few letters);
/// with less room it leaves the row whole rather than show a sliver. A
/// reading measure, off the space scale like `NAV_TITLE_FLOOR`.
pub const NAV_BRANCH_MIN_W: f32 = 40.0;
/// 55px — the tail's box: `needs you` at `FS_SM` in Geist (54.7px, ceiled),
/// the longest word the tail says, so no word arriving moves the title.
pub const NAV_TAIL_MIN_W: f32 = 55.0;
/// 254px — the content box of a root-level nav row: the column less the
/// tree's inline padding, less the row's own. A truncating title has to be
/// pinned to it, because gpui only measures an ellipsis against a width it
/// knows on the line's very first measure (see `nav::group_row`).
pub const ROW_TEXT_W: f32 = NAV_WIDTH - 2.0 * NAV_TREE_PAD - 2.0 * ROW_PAD_X;
/// 28px — a section heading's row (a Project heading, the Parked header):
/// one metadata line in the rows' own padding.
pub const NAV_SECTION_H: f32 = 2.0 * ROW_PAD_Y + LH_META;
/// 12px (`GAP_BLOCK`) — every block boundary in the tree: between two
/// Group blocks (a drop band as well as air), above a run of solos after a
/// Group, above a section heading. Rows inside a block sit flush: the 28px
/// row carries its own air.
pub const GROUP_GAP: f32 = GAP_BLOCK;
pub const SOLOS_TOP: f32 = GROUP_GAP;
/// A Group row's members hang flush under it and flush with each other.
pub const MEMBERS_TOP: f32 = 0.0;
pub const MEMBER_GAP: f32 = 0.0;
/// 4px — a drop target that takes no layout of its own ("insert above the
/// first Group", "append after the last member"): an absolute hit band
/// over the edge of the row it borders.
pub const NAV_DROP_BAND: f32 = SPACE_1;
/// 22px — the member indent: a member's lead slot starts under its Group's
/// title, the tree grammar of a child's marker under its parent's text.
pub const MEMBER_INDENT: f32 = NAV_TEXT_X;
/// The 1px rail hangs from the Group glyph's centre: `RAIL_OFFSET` left of
/// the members box, inset 3px top and bottom. Translucent — draw it with
/// `rgba`.
pub const RAIL_OFFSET: f32 = MEMBER_INDENT - ROW_PAD_X - NAV_LEAD_W / 2.0;
pub const RAIL_INSET: f32 = 3.0;
pub const NAV_GROUP_RAIL: u32 = HAIRLINE;
/// A nav row's radius: a menu row's (`R_MENU_ROW`), since the nav is a list
/// of the same 28px rows.
pub const NAV_ROW_R: f32 = R_MENU_ROW;
/// 12px — above a section heading (a Project, the Parked fold).
pub const NAV_SECTION_GAP: f32 = GROUP_GAP;
/// 12px — the provider logomark in a nav row and a rail item, in its own
/// fixed slot at the row's right.
pub const PROVIDER_MARK: f32 = GLYPH_BOX;
/// How far a rail item's status dot sits in from its box's corner, and its
/// ordinal (1–9, the ⌘1…9 it answers to) from the opposite one.
pub const NAV_RAIL_DOT_INSET: f32 = SPACE_1;
/// The collapsed rail. On macOS its controls are 36px — the rail owns the
/// traffic lights' 77px reserve, and a 28px control would float in it —
/// and its first control starts below the native lights' band. Elsewhere
/// the rail keeps the compact `ICON_BUTTON` and an 8px inset.
pub const NAV_RAIL_CONTROL: f32 = if cfg!(target_os = "macos") {
    36.0
} else {
    ICON_BUTTON
};
pub const NAV_RAIL_CHROME_PAD_T: f32 = if cfg!(target_os = "macos") {
    WIN_CHROME_H
} else {
    SPACE_2
};
pub const NAV_RAIL_CHROME_PAD_B: f32 = SPACE_1;
/// The rail's own block padding, the gap between its items, and the gap
/// above its first item (also the empty-filter message's block margin).
pub const NAV_RAIL_PAD_Y: f32 = SPACE_2;
pub const NAV_RAIL_ITEM_GAP: f32 = SPACE_1;
pub const NAV_RAIL_ITEMS_TOP: f32 = SPACE_3;
/// The most of the column the open Parked section may take. Its list
/// scrolls past this, so a hundred parked Threads never push the running
/// tree out of sight.
pub const NAV_PARKED_MAX_SHARE: f32 = 0.5;
/// Narrow windows fold the nav to the rail (rule 2.7.7) — shown, never
/// saved — once the board beside the full column would be narrower than
/// this, or a board cell narrower than the L2 floor (`INSTRUMENTS_WIDTH`).
/// It unfolds only once the window clears the threshold by
/// `NAV_AUTO_RAIL_HYSTERESIS`, so a resize at the edge never flickers it.
/// cmd-B still overrides it either way.
pub const NAV_AUTO_RAIL_BOARD_W: f32 = 560.0;
pub const NAV_AUTO_RAIL_HYSTERESIS: f32 = 24.0;
// (end WP-G) — append above this line only

// ======================================== motion
// Owner: the motion kit (motion.rs, and the call sites that ride it).
// Edit values and append tokens only inside this section.
//
// **Motion is a budget, not a garnish.** The catalog is Zeron's, ported
// with its numbers (`motion.rs` maps each entry to the Ferrite surface
// that wears it). The rules every animated surface follows:
//
// - **Ease out, and exits softer than entrances.** Entrances rise or settle
//   a few pixels into place. A menu or a sheet that closes goes at once:
//   dismissals are frequent, and the closed state says it all. A fold grows
//   open under its header (`MOTION_COLLAPSE_MS`) and shuts at once; the
//   sidebar's width rides cmd-B (`MOTION_RESIZE_MS`), interruptibly.
// - **Pointer hover gets one 150ms colour blend (`MOTION_HOVER_FADE_MS`).**
//   A keyboard change (focus, cmd-] / cmd-D, menu selection, folds, tier
//   changes, a Send becoming ready) and a press land on the same frame.
//   A transcript row appended live rises in (`MOTION_FADE_IN_MS` 500, 4px);
//   first paint and scroll-back never animate. Nothing on a high-frequency
//   interaction scales, slides or staggers.
// - **Little moves at once** (theme rule 8). The loops: the braille spinner
//   in place of a working Thread's dot (`MOTION_BRAILLE_FRAME_MS`), the
//   working line's glyph spinner (`MOTION_WORKING_FRAME_MS`) and its
//   caption's shimmer (`MOTION_SHIMMER_MS`) on the focused Pane, and the
//   focused Composer's caret blink (`MOTION_CARET_BLINK_MS`). No dot pulses
//   or breathes: `MOTION_BREATH_MS` is legacy.
// - **Interruptible.** A state the operator can flip back (a hover, a
//   chevron) retargets from where it is, never restarts.
// - **No entrance on first paint.** Only a change the operator watched
//   happen animates; a restored layout or scroll-back arrives in place.
// - **Every animation has a static end state** that says the same thing
//   without motion. Reduced motion (the system flag, `cx.reduce_motion()`)
//   snaps a one-shot to its end and holds a loop at its start.
// - **A window with nothing animating schedules no frame.** Loops ride one
//   shared, throttled pulse clock (`MOTION_PULSE_TICK_MS`), leased by the
//   views that paint them; it parks when the last lease lapses.

/// Zeron's signature entrance curve, CSS `cubic-bezier(0.16, 1, 0.3, 1)`.
pub const MOTION_EASE_OUT_EXPO: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
/// CSS `ease`: quick fades, menu and sheet entrances.
pub const MOTION_EASE: [f32; 4] = [0.25, 0.1, 0.25, 1.0];
/// CSS `transition-colors`' default curve: every hover blend.
pub const MOTION_EASE_STANDARD: [f32; 4] = [0.4, 0.0, 0.2, 1.0];
/// CSS `ease-out`: the sidebar's width and the Parked fold.
pub const MOTION_EASE_OUT: [f32; 4] = [0.0, 0.0, 0.58, 1.0];
/// A contextual icon swap's curve (a spring with no bounce, approximated).
pub const MOTION_EASE_ICON: [f32; 4] = [0.2, 0.0, 0.0, 1.0];
/// `row-in`: 180ms on `MOTION_EASE_OUT_EXPO`, opacity only, nothing moves:
/// a line arriving inside a card already open (the usage card's legend).
pub const MOTION_ROW_IN_MS: u64 = 180;
/// Zeron's `fade-in`: a transcript row appended live fades up over 500ms
/// while rising `MOTION_FADE_IN_RISE` into place.
pub const MOTION_FADE_IN_MS: u64 = 500;
pub const MOTION_FADE_IN_RISE: f32 = 4.0;
/// The sidebar's width between column and rail (cmd-B), and the Parked
/// fold growing open under its header.
pub const MOTION_RESIZE_MS: u64 = 200;
pub const MOTION_COLLAPSE_MS: u64 = 180;
/// Where the sidebar's content fades up from while its width moves.
pub const MOTION_NAV_CONTENT_FROM: f32 = 0.35;
/// `fade-quick`: 150ms, opacity only.
pub const MOTION_FADE_QUICK_MS: u64 = 150;
/// `menu-in`: 140ms, settling 2px away from its opener (Zeron's 0.96 scale
/// has no div transform here; the shift and fade carry it). A menu closes
/// at once: no exit.
pub const MOTION_MENU_IN_MS: u64 = 140;
pub const MOTION_MENU_SHIFT: f32 = 2.0;
/// The opacity a menu starts from: it is already legible on its first frame.
pub const MOTION_MENU_FROM_OPACITY: f32 = 0.3;
/// `dialog-in`: 180ms, rising 2px (the 0.96 scale approximated likewise).
pub const MOTION_DIALOG_IN_MS: u64 = 180;
pub const MOTION_DIALOG_RISE: f32 = 2.0;
/// How far a board slot opening beside its owner (the reader) settles in
/// from the owner's side.
pub const MOTION_SLOT_SHIFT: f32 = 6.0;
/// A disclosure chevron turning: 150ms.
pub const MOTION_CHEVRON_MS: u64 = 150;
/// The hover blend: 150ms on `MOTION_EASE_STANDARD`.
pub const MOTION_HOVER_FADE_MS: u64 = 150;
/// 2.4s — the one breath (rule 2.10.3): unread breathing on a head dot
/// reads `motion::pulse_phase` on this period, so every breathing dot on
/// screen shares one ~30fps tick. Held at its start under reduced motion.
pub const MOTION_BREATH_MS: u64 = 2_400;
/// A contextual icon swap (send ⇄ stop): 300ms, the leaving glyph shrinking
/// to a quarter as the arriving one grows from it.
pub const MOTION_ICON_SWAP_MS: u64 = 300;
pub const MOTION_ICON_SWAP_SCALE: f32 = 0.25;
/// A toast's stack timing: it settles in over 180ms and leaves over 100ms.
pub const MOTION_TOAST_IN_MS: u64 = 180;
pub const MOTION_TOAST_OUT_MS: u64 = 100;
/// 1.4s — how long a scrollbar thumb holds after the last scroll frame
/// (C26) before it fades out over `MOTION_HOVER_FADE_MS`. Idle, no thumb.
pub const MOTION_SCROLLBAR_LINGER_MS: u64 = 1_400;
/// The pulse clock: one ~30fps tick shared by every loop in the window.
/// A view stays on it `MOTION_PULSE_LEASE_MS` after its last paint of a
/// loop, so an unmounted loader drops off and the clock parks.
pub const MOTION_PULSE_TICK_MS: u64 = 33;
pub const MOTION_PULSE_LEASE_MS: u64 = 300;
/// 80ms — one frame of the braille spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, ten frames, an
/// 800ms turn) that stands in a working Thread's dot.
#[allow(dead_code)]
pub const MOTION_BRAILLE_FRAME_MS: u64 = 80;
/// 120ms — one frame of the working line's spinner (`· ✢ ✳ ✶ ✻ ✽ ✻ ✶ ✳ ✢`,
/// ten frames, 1.2s), Claude Code's own cadence.
#[allow(dead_code)]
pub const MOTION_WORKING_FRAME_MS: u64 = 120;
/// 2.4s — one sweep of the shimmer's crest across the working caption,
/// left to right.
#[allow(dead_code)]
pub const MOTION_SHIMMER_MS: u64 = 2_400;
/// The crest's half-width, as a fraction of the caption's width (the
/// prototype's 35%–50%–65% stops over a 300% gradient).
#[allow(dead_code)]
pub const SHIMMER_HALF_WIDTH: f32 = 0.45;
/// 1.1s — the focused Composer's soft block caret: full for 45% of the
/// period, eased down to `CARET_BLINK_MIN` and held, eased back up.
#[allow(dead_code)]
pub const MOTION_CARET_BLINK_MS: u64 = 1_100;
#[allow(dead_code)]
pub const CARET_BLINK_MIN: f32 = 0.12;
// (end motion) — append above this line only

#[cfg(test)]
mod tests {
    use super::*;

    /// Rule 2.2.4: no state tints a ground. The Decision's wash is retired,
    /// and nothing in the crate may bring it back.
    #[test]
    fn no_source_file_names_the_retired_attention_wash() {
        let retired = ["ATTENTION", "_WASH"].concat();
        let mut stack = vec![std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src"
        ))];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let source = std::fs::read_to_string(&path).unwrap();
                    assert!(!source.contains(&retired), "{path:?} names {retired}");
                }
            }
        }
    }

    /// Every lexicon word, so the casing rule below covers them all.
    const LEXICON: &[&str] = &[
        words::NEEDS_YOU,
        words::APPROVAL,
        words::QUESTION,
        words::DONE,
        words::FAILED,
        words::FAILING,
        words::INTERRUPTED,
        words::WORKING,
        words::STARTING,
        words::PAUSED,
        words::UNAVAILABLE,
    ];

    #[test]
    fn every_state_word_is_lowercase_and_dashless() {
        for word in LEXICON {
            assert!(!word.is_empty());
            assert_eq!(*word, word.to_lowercase(), "{word:?} is not lowercase");
            assert!(!word.contains('\u{2014}'), "{word:?} carries an em dash");
            assert!(!word.contains('\u{b7}'), "{word:?} carries a separator");
            assert_eq!(*word, word.trim(), "{word:?} carries padding");
        }
    }

    #[test]
    fn only_needs_you_and_failure_words_take_a_colour() {
        for word in LEXICON {
            let expected = match *word {
                words::NEEDS_YOU => ATTENTION,
                words::FAILED | words::FAILING => BLOCKED,
                _ => TEXT_MUTED,
            };
            assert_eq!(word_ink(word), expected, "{word:?}");
        }
        assert_eq!(word_ink("needs you"), ATTENTION);
        assert_eq!(word_ink("failed"), BLOCKED);
        assert_eq!(word_ink("failing"), BLOCKED);
        assert_eq!(word_ink("done"), TEXT_MUTED);
    }

    /// WCAG 2.x relative luminance of an opaque `0xRRGGBB`.
    fn luminance(rgb: u32) -> f32 {
        let channel = |shift: u32| {
            let c = ((rgb >> shift) & 0xff) as f32 / 255.;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    /// `0xRRGGBBAA` laid over an opaque plane, as the eye sees it.
    fn over(rgba: u32, plane: u32) -> u32 {
        let alpha = (rgba & 0xff) as f32 / 255.;
        let mix = |shift: u32| {
            let top = ((rgba >> (shift + 8)) & 0xff) as f32;
            let bottom = ((plane >> shift) & 0xff) as f32;
            ((top * alpha + bottom * (1. - alpha)).round() as u32) << shift
        };
        mix(16) | mix(8) | mix(0)
    }

    fn contrast(ink: u32, plane: u32) -> f32 {
        let (a, b) = (luminance(ink), luminance(plane));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn floor(inks: &[(&str, u32)], planes: &[(&str, u32)], min: f32) {
        for (ink_name, ink) in inks {
            for (plane_name, plane) in planes {
                let ratio = contrast(*ink, *plane);
                assert!(
                    ratio >= min,
                    "{ink_name} on {plane_name} is {ratio:.2}:1, below {min}:1"
                );
            }
        }
    }

    /// The planes a readable row rests on (rule 5).
    const PLANES: &[(&str, u32)] = &[
        ("PLANE", PLANE),
        ("CHROME", CHROME),
        ("FLOAT", FLOAT),
        ("HOVER", HOVER),
        ("BAND", BAND),
        ("HUNK", HUNK),
        ("NODIFF", NODIFF),
    ];

    #[test]
    fn ink_clears_its_floor_on_every_plane() {
        // Readable text: AA body text on every plane a row rests on.
        floor(
            &[
                ("TEXT_STRONG", TEXT_STRONG),
                ("TEXT", TEXT),
                ("TEXT_2", TEXT_2),
                ("TEXT_MUTED", TEXT_MUTED),
            ],
            PLANES,
            4.5,
        );
        // A selected row and a band2 chip: the operator's dim (#98989d)
        // holds 4:1 there; the brighter inks hold AA.
        let raised = &[("SELECTION", SELECTION), ("BAND2", BAND2)];
        floor(&[("TEXT_MUTED", TEXT_MUTED)], raised, 4.0);
        floor(&[("TEXT_STRONG", TEXT_STRONG), ("TEXT", TEXT)], raised, 4.5);
        // Structure only, never text: a visible glyph on its planes.
        floor(
            &[("TEXT_FAINT", TEXT_FAINT)],
            &[
                ("PLANE", PLANE),
                ("CHROME", CHROME),
                ("FLOAT", FLOAT),
                ("BAND", BAND),
            ],
            2.0,
        );
        // Every terminal colour reads as text on the planes it is written on.
        floor(
            &[
                ("ACCENT", ACCENT),
                ("RUNNING", RUNNING),
                ("ATTENTION", ATTENTION),
                ("BLOCKED", BLOCKED),
                ("SYN_KEYWORD", SYN_KEYWORD),
                ("SYN_FUNCTION", SYN_FUNCTION),
                ("SYN_TYPE", SYN_TYPE),
                ("SYN_STRING", SYN_STRING),
                ("SYN_NUMBER", SYN_NUMBER),
                ("SYN_CONST", SYN_CONST),
                ("SYN_COMMENT", SYN_COMMENT),
                ("SYN_PUNCT", SYN_PUNCT),
                ("SYN_PLAIN", SYN_PLAIN),
                ("INLINE_CODE", INLINE_CODE),
                ("PATH_INK", PATH_INK),
                ("MODE_INK", MODE_INK),
                ("DIFF_ADDED_INK", DIFF_ADDED_INK),
                ("DIFF_REMOVED_INK", DIFF_REMOVED_INK),
            ],
            &[
                ("PLANE", PLANE),
                ("CHROME", CHROME),
                ("FLOAT", FLOAT),
                ("BAND", BAND),
            ],
            4.5,
        );
        // Code on a diff row's wash, and the changed words' deeper wash.
        for wash in [
            DIFF_ADDED_WASH,
            DIFF_REMOVED_WASH,
            DIFF_ADDED_WORD,
            DIFF_REMOVED_WORD,
        ] {
            floor(
                &[("TEXT", TEXT), ("SYN_FUNCTION", SYN_FUNCTION)],
                &[("a diff wash on PLANE", over(wash, PLANE))],
                4.5,
            );
        }
        // A primary button's label, at rest, under the pointer and pressed.
        floor(
            &[("ON_ACCENT", ON_ACCENT)],
            &[
                ("ACCENT_STRONG", ACCENT_STRONG),
                ("PRIMARY_HOVER", PRIMARY_HOVER),
                ("PRIMARY_ACTIVE", PRIMARY_ACTIVE),
            ],
            4.5,
        );
        // Focus and control boundaries, and a provider's mark: non-text 3:1.
        floor(
            &[
                ("FOCUS_RING", FOCUS_RING),
                ("INPUT_EDGE", INPUT_EDGE),
                ("PROVIDER_CLAUDE", PROVIDER_CLAUDE),
                ("PROVIDER_CODEX", PROVIDER_CODEX),
            ],
            &[
                ("PLANE", PLANE),
                ("CHROME", CHROME),
                ("FLOAT", FLOAT),
                ("BAND", BAND),
            ],
            3.0,
        );
        // Ink stays readable on the washes painted under it.
        floor(
            &[("TEXT_STRONG", TEXT_STRONG)],
            &[("ACCENT_WASH on PLANE", over(ACCENT_WASH, PLANE))],
            4.5,
        );
        floor(
            &[("TEXT", TEXT)],
            &[(
                "TEXT_SELECTION_WASH on PLANE",
                over(TEXT_SELECTION_WASH, PLANE),
            )],
            4.5,
        );
    }

    /// Rule 5: four inks, brightest first (`TEXT_2` is a legacy alias held
    /// between `TEXT` and `TEXT_MUTED` so nothing collapses mid-restyle).
    #[test]
    fn the_ink_ladder_steps_down() {
        let ladder = [TEXT_STRONG, TEXT, TEXT_2, TEXT_MUTED, TEXT_FAINT];
        for pair in ladder.windows(2) {
            assert!(luminance(pair[0]) > luminance(pair[1]), "{pair:06x?}");
        }
    }

    /// Rule 3: grey, not near-black — the opaque ladder in order, every
    /// face opaque (`rgb`), hover visible on every ground it hovers.
    #[test]
    fn the_plane_ladder_is_strictly_ordered() {
        let ladder = [
            ("NODIFF", NODIFF),
            ("PLANE", PLANE),
            ("HUNK", HUNK),
            ("CHROME", CHROME),
            ("FLOAT", FLOAT),
            ("HOVER", HOVER),
            ("BAND", BAND),
            ("SELECTION", SELECTION),
            ("BAND2", BAND2),
            ("SELECTION_HOVER", SELECTION_HOVER),
        ];
        for pair in ladder.windows(2) {
            assert!(
                luminance(pair[0].1) < luminance(pair[1].1),
                "{} must sit below {}",
                pair[0].0,
                pair[1].0
            );
        }
        assert_eq!(
            (HEAD, INBAND),
            (CHROME, CHROME),
            "the bands sit on chrome's value"
        );
        assert!(luminance(LINE) > luminance(PLANE) && luminance(LINE2) > luminance(LINE));
        for (_, face) in ladder {
            assert!(face <= 0xffffff, "{face:x} carries alpha");
        }
        // The legacy names land on the new ladder.
        assert_eq!((GROUND, NAV, PANE), (CHROME, CHROME, PLANE));
        assert_eq!(
            (FILL, FILL_HOVER, HOVER_RAISED),
            (SELECTION, SELECTION_HOVER, HOVER)
        );
    }

    /// Rule 4: glass on macOS — the window paints nothing, chrome and the
    /// plane are translucent at their stated opacities, overlays are white,
    /// a float stays near-opaque — and the opaque greys everywhere else.
    #[test]
    fn the_paints_are_glass_on_macos_and_grey_elsewhere() {
        let alpha = |rgba: u32| (rgba & 0xff) as f32 / 255.;
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(near(alpha(PLANE_GLASS), 0.94));
        assert!(near(alpha(CHROME_GLASS), 0.72));
        assert!(near(alpha(BAND_GLASS), 0.80));
        assert!(near(alpha(FLOAT_GLASS), 0.96));
        for (overlay, opacity) in [
            (HEAD_GLASS, 0.05),
            (INBAND_GLASS, 0.05),
            (HOVER_GLASS, 0.06),
            (BAND2_GLASS, 0.10),
            (SELECTION_GLASS, 0.11),
            (SELECTION_HOVER_GLASS, 0.14),
            (LINE_GLASS, 0.08),
            (LINE2_GLASS, 0.14),
        ] {
            assert_eq!(overlay >> 8, 0xffffff, "{overlay:08x} is a white overlay");
            assert!(near(alpha(overlay), opacity), "{overlay:08x}");
        }
        // Glass overlays step up in the same order as the opaque ladder.
        assert!(HEAD_GLASS & 0xff < HOVER_GLASS & 0xff);
        assert!(HOVER_GLASS & 0xff < SELECTION_GLASS & 0xff);
        assert!(SELECTION_GLASS & 0xff < SELECTION_HOVER_GLASS & 0xff);
        let paints = [
            (paint::PLANE, PLANE_GLASS, PLANE),
            (paint::CHROME, CHROME_GLASS, CHROME),
            (paint::HEAD, HEAD_GLASS, HEAD),
            (paint::INBAND, INBAND_GLASS, INBAND),
            (paint::BAND, BAND_GLASS, BAND),
            (paint::BAND2, BAND2_GLASS, BAND2),
            (paint::HOVER, HOVER_GLASS, HOVER),
            (paint::SELECTION, SELECTION_GLASS, SELECTION),
            (
                paint::SELECTION_HOVER,
                SELECTION_HOVER_GLASS,
                SELECTION_HOVER,
            ),
            (paint::FLOAT, FLOAT_GLASS, FLOAT),
            (paint::HUNK, HUNK_GLASS, HUNK),
            (paint::NODIFF, NODIFF_GLASS, NODIFF),
            (paint::LINE, LINE_GLASS, LINE),
            (paint::LINE2, LINE2_GLASS, LINE2),
        ];
        for (paint, glass, opaque) in paints {
            if GLASS {
                assert_eq!(paint.rgba(), glass);
                assert!(!paint.is_opaque());
            } else {
                assert_eq!(paint.rgba(), solid(opaque));
                assert!(paint.is_opaque());
            }
        }
        assert_eq!(paint::PRESS, paint::SELECTION_HOVER);
        if GLASS {
            assert_eq!(paint::WINDOW.rgba(), TRANSPARENT, "the root paints nothing");
            assert_eq!(paint::CHROME_SEAM.rgba(), CHROME_SEAM_GLASS);
            assert_eq!(
                window_background(),
                gpui::WindowBackgroundAppearance::Blurred
            );
        } else {
            assert_eq!(paint::WINDOW.rgba(), solid(PLANE));
            assert_eq!(paint::CHROME_SEAM.rgba(), TRANSPARENT);
            assert_eq!(
                window_background(),
                gpui::WindowBackgroundAppearance::Opaque
            );
        }
        // The legacy line names follow the platform's lines.
        assert_eq!((HAIRLINE, HAIRLINE_STRONG), (paint::LINE.0, paint::LINE2.0));
        // A paint converts the way `rgba()` does, never the way `rgb()` would.
        let fill: gpui::Fill = paint::PLANE.into();
        assert_eq!(fill, gpui::Fill::from(gpui::rgba(paint::PLANE.0)));
        let hsla: gpui::Hsla = paint::HOVER.into();
        assert_eq!(hsla, paint::HOVER.hsla());
    }

    /// Rule 2: square inside, and nothing lit.
    #[test]
    fn every_radius_is_square_and_nothing_is_lit() {
        for radius in [R_PANE, R_BLOCK, R_CONTROL, R_CHIP, R_TIGHT, R_MENU_ROW] {
            assert_eq!(radius, 0.0);
        }
        for light in [LIGHT_LOW, LIGHT_HIGH, KEY_FOOT, WELL_SHADE] {
            assert_eq!(light, TRANSPARENT);
        }
        assert!(SHADOW_FLOAT_Y > 0.0 && SHADOW_FLOAT_BLUR > 0.0);
    }

    #[test]
    fn nav_row_heights_are_derived() {
        assert_eq!(NAV_ROW_H, 2.0 * SPACE_1 + LH_UI);
        assert_eq!(THREAD_ROW_H, NAV_ROW_H);
        assert_eq!(GROUP_ROW_H, NAV_ROW_H);
        assert_eq!(THREAD_ROW_H, MENU_ROW_H, "a nav row is a menu row's box");
        assert_eq!(THREAD_ROW_H, 28.0, "one list pitch across the app (C9)");
    }

    /// Rule 1: one size per surface on a whole-pixel line, a 1.5× terminal
    /// row at every reading size, headings at the body size, a cell of
    /// 0.6em.
    #[test]
    fn every_type_role_has_a_whole_pixel_line_box() {
        use ferrite_core::settings::ReadingSize;
        assert_eq!((FS_UI, LH_UI), (13.0, 20.0), "the grid is 13 on 20");
        for (size, line) in [(FS_PROSE, LH_PROSE), (FS_UI, LH_UI)] {
            assert_eq!(line, line.round());
            assert!(line >= size * 1.5, "{size}px on a {line}px line box");
        }
        // The legacy roles are the one size now.
        assert_eq!((FS_SM, FS_PROSE_SM), (FS_UI, FS_UI));
        assert_eq!((LH_META, LH_PROSE_SM, LH_CODE), (LH_UI, LH_UI, LH_UI));
        for reading in ReadingSize::STEPS.map(ReadingSize::nearest) {
            let (size, line) = (answer_text_size(reading), answer_line_height(reading));
            assert_eq!(line, line.round());
            assert!(line >= size * 1.5, "{reading:?}: {size}/{line}");
            assert!(line <= size * 1.55, "{reading:?}: {size}/{line}");
        }
        assert_eq!(answer_line_height(ReadingSize::nearest(13)), LH_UI);
        assert_eq!(answer_text_size(ReadingSize::STANDARD), FS_PROSE);
        assert_eq!(answer_line_height(ReadingSize::STANDARD), LH_PROSE);
        for level in 1..=6 {
            assert_eq!(heading_scale(level), 1.0, "H{level} is the body size");
        }
        assert_eq!(prose_line_height(FS_PROSE), LH_PROSE);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(near(CODE_CELL, 7.8) && near(CH, CODE_CELL));
        assert!(near(GLYPH_GUTTER, 2.0 * CH));
        assert_eq!((ROW, HALF_ROW), (20.0, 10.0));
        assert_eq!((TITLEBAR_H, STATUS_BAR_H), (32.0, 24.0));
        assert_eq!(WIN_CHROME_H, TITLEBAR_H);
        assert_eq!(TRAFFIC_Y, (WIN_CHROME_H - 14.0) / 2.0);
    }

    #[gpui::test]
    fn kit_tokens_follow_ferrite_roles(cx: &mut gpui::TestAppContext) {
        use gpui::{component::Theme, rgb, rgba, Hsla};
        cx.update(|cx| {
            init_components(cx);
            let theme = Theme::global(cx);
            let solid = |value: u32| -> Hsla { rgb(value).into() };
            let alpha = |value: u32| -> Hsla { rgba(value).into() };
            // The resolved tokens, which kit widgets actually paint from.
            let tokens = &theme.tokens;
            assert_eq!(tokens.button_primary.color, solid(ACCENT_STRONG));
            assert_eq!(tokens.button_primary_hover.color, solid(PRIMARY_HOVER));
            assert_eq!(tokens.button_primary_foreground.color, solid(ON_ACCENT));
            assert_eq!(tokens.primary.color, solid(ACCENT_STRONG));
            assert_eq!(tokens.accent.color, paint::SELECTION.hsla());
            assert_eq!(tokens.popover.color, paint::FLOAT.hsla());
            assert_eq!(tokens.muted.color, solid(BAND));
            assert_eq!(tokens.tab_active.color, solid(BAND2));
            assert_eq!(tokens.ring.color, solid(FOCUS_RING));
            assert_eq!(tokens.input.color, solid(INPUT_EDGE));
            assert_eq!(tokens.border.color, paint::LINE2.hsla());
            assert_eq!(tokens.selection.color, alpha(TEXT_SELECTION_WASH));
            assert_eq!(tokens.caret.color, solid(ACCENT));
            assert_eq!(tokens.link.color, solid(ACCENT));
            assert_eq!(tokens.sidebar.color, solid(CHROME));
            // And the colours the non-token paths read.
            assert_eq!(theme.primary, solid(ACCENT_STRONG));
            assert_eq!(theme.muted, solid(BAND));
            assert_eq!(theme.background, solid(PLANE));
            assert!(!theme.shadow, "floats wear Ferrite's own shadow");
            assert_eq!(theme.radius, gpui::px(0.));
            assert_eq!(theme.radius_lg, gpui::px(0.));
            assert_eq!(theme.font_size, gpui::px(FS_UI));
            assert_eq!(theme.mono_font_size, gpui::px(FS_UI));
            assert_eq!(theme.font_family.as_ref(), "Geist Mono");
            assert_eq!(theme.mono_font_family.as_ref(), "Geist Mono");
        });
    }

    /// A TrueType table's byte range, by tag.
    fn table<'a>(font: &'a [u8], tag: &[u8; 4]) -> &'a [u8] {
        let u16_at = |at: usize| u16::from_be_bytes([font[at], font[at + 1]]) as usize;
        let u32_at = |at: usize| {
            u32::from_be_bytes([font[at], font[at + 1], font[at + 2], font[at + 3]]) as usize
        };
        (0..u16_at(4))
            .map(|i| 12 + 16 * i)
            .find(|record| &font[*record..*record + 4] == tag)
            .map(|record| &font[u32_at(record + 8)..][..u32_at(record + 12)])
            .unwrap_or_else(|| panic!("no {} table", String::from_utf8_lossy(tag)))
    }

    /// Rule 2.1.7 of the type: copy equals what is drawn, so the code face
    /// must not ship ligatures (`calt`/`liga`) that would draw `->` or `!=`
    /// as one glyph. Geist Mono ships none (its GSUB carries only case,
    /// fractions, ordinals and stylistic sets), so no feature needs turning
    /// off; this pins that, and a face update that adds them fails here.
    #[test]
    fn the_code_face_ships_no_ligatures() {
        for (face, font) in [("Geist Mono", GEIST_MONO)] {
            let gsub = table(font, b"GSUB");
            let features = &gsub[be16(gsub, 6) as usize..];
            let tags: Vec<String> = (0..be16(features, 0) as usize)
                .map(|i| String::from_utf8_lossy(&features[2 + 6 * i..][..4]).into_owned())
                .collect();
            assert!(!tags.is_empty(), "{face}: read its GSUB features");
            for ligature in ["calt", "liga", "dlig"] {
                assert!(
                    !tags.iter().any(|tag| tag == ligature),
                    "{face} ships `{ligature}`: turn it off on every code run ({tags:?})"
                );
            }
        }
    }

    fn be16(data: &[u8], at: usize) -> u32 {
        u16::from_be_bytes([data[at], data[at + 1]]) as u32
    }

    fn be32(data: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
    }

    /// Whether the face's Unicode cmap (format 12, else format 4) maps `c`
    /// to a real glyph.
    fn covers(font: &[u8], c: char) -> bool {
        let cmap = table(font, b"cmap");
        let code = c as u32;
        let subtables: Vec<&[u8]> = (0..be16(cmap, 2) as usize)
            .map(|i| &cmap[be32(cmap, 4 + 8 * i + 4) as usize..])
            .collect();
        if let Some(sub) = subtables.iter().find(|sub| be16(sub, 0) == 12) {
            return (0..be32(sub, 12) as usize).any(|group| {
                let at = 16 + 12 * group;
                (be32(sub, at)..=be32(sub, at + 4)).contains(&code) && {
                    be32(sub, at + 8) + code - be32(sub, at) != 0
                }
            });
        }
        let sub = subtables
            .iter()
            .find(|sub| be16(sub, 0) == 4)
            .expect("a Unicode cmap");
        let segments = be16(sub, 6) as usize / 2;
        let (ends, starts) = (14, 16 + 2 * segments);
        let (deltas, ranges) = (starts + 2 * segments, starts + 4 * segments);
        (0..segments).any(|seg| {
            let (start, end) = (be16(sub, starts + 2 * seg), be16(sub, ends + 2 * seg));
            if !(start..=end).contains(&code) {
                return false;
            }
            let delta = be16(sub, deltas + 2 * seg);
            let range = be16(sub, ranges + 2 * seg) as usize;
            let glyph = if range == 0 {
                code
            } else {
                let at = ranges + 2 * seg + range + 2 * (code - start) as usize;
                be16(sub, at)
            };
            glyph != 0 && (glyph + delta) & 0xffff != 0
        })
    }

    /// The typographic family (name ID 16), or the family (ID 1).
    fn family(font: &[u8]) -> String {
        let name = table(font, b"name");
        let strings = be16(name, 4) as usize;
        let records: Vec<(u32, u32, usize, usize)> = (0..be16(name, 2) as usize)
            .map(|i| 6 + 12 * i)
            .map(|at| {
                (
                    be16(name, at),
                    be16(name, at + 6),
                    be16(name, at + 8) as usize,
                    be16(name, at + 10) as usize,
                )
            })
            .collect();
        [16, 1]
            .into_iter()
            .find_map(|id| {
                records
                    .iter()
                    .find(|(platform, name_id, ..)| *platform == 3 && *name_id == id)
            })
            .map(|(_, _, len, offset)| {
                let bytes = &name[strings + offset..][..*len];
                String::from_utf16_lossy(
                    &bytes
                        .chunks(2)
                        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                        .collect::<Vec<_>>(),
                )
            })
            .expect("a Windows family name")
    }

    const GEIST_MONO: &[u8] = include_bytes!("../assets/fonts/GeistMono.ttf");

    /// Rule 1: one face. Both role names resolve to Geist Mono, and every
    /// bundled face is one of its weights.
    #[test]
    fn faces_are_the_bundled_family() {
        assert_eq!(FONT_UI, "Geist Mono");
        assert_eq!(FONT_CODE, FONT_UI, "one face on every surface");
        assert!(!crate::FONTS.is_empty());
        for face in crate::FONTS {
            let family = family(face);
            assert_eq!(
                family, FONT_UI,
                "a bundled face names the family `{family}`"
            );
        }
        assert_eq!(family(GEIST_MONO), FONT_UI);
    }

    /// Rule 10: what text may use is in the face; what the design draws is
    /// not, and stays an SVG.
    #[test]
    fn chrome_glyphs_are_in_the_face_and_drawn_glyphs_are_not() {
        assert!(covers(GEIST_MONO, 'a') && covers(GEIST_MONO, '$'));
        for glyph in CHROME_GLYPHS {
            assert!(covers(GEIST_MONO, *glyph), "{glyph} is not in Geist Mono");
        }
        for glyph in DRAWN_GLYPHS {
            assert!(
                !covers(GEIST_MONO, *glyph),
                "{glyph} is in Geist Mono now: it may be text (move it to CHROME_GLYPHS)"
            );
        }
        // The spinner frames the design animates are all drawn.
        for frame in [
            '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '✢', '✳', '✶', '✻', '✽',
        ] {
            assert!(DRAWN_GLYPHS.contains(&frame), "{frame}");
        }
    }

    /// Every non-ASCII glyph render code puts in a literal must be one the
    /// face draws: `❯ ⎿ ∴ ✻ ✓ ✗ ☐ ◆ ⌘` and friends are SVG glyph boxes or
    /// painted marks. Scans the render modules' non-test source,
    /// skipping comments.
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        let sources: &[(&str, &str)] = &[
            ("pane.rs", include_str!("pane.rs")),
            ("pane/text.rs", include_str!("pane/text.rs")),
            ("cockpit.rs", include_str!("cockpit.rs")),
            ("cockpit/subagents.rs", include_str!("cockpit/subagents.rs")),
            ("transcript.rs", include_str!("transcript.rs")),
            ("rich.rs", include_str!("rich.rs")),
            ("nav.rs", include_str!("nav.rs")),
            ("titlebar.rs", include_str!("titlebar.rs")),
            ("menu.rs", include_str!("menu.rs")),
            ("composer.rs", include_str!("composer.rs")),
            ("components.rs", include_str!("components.rs")),
            ("prefs.rs", include_str!("prefs.rs")),
            ("project_editor.rs", include_str!("project_editor.rs")),
            ("notifications.rs", include_str!("notifications.rs")),
            ("attachments.rs", include_str!("attachments.rs")),
            ("background_chips.rs", include_str!("background_chips.rs")),
            ("keymap.rs", include_str!("keymap.rs")),
            ("decision.rs", include_str!("decision.rs")),
            (
                "attachment_preview.rs",
                include_str!("attachment_preview.rs"),
            ),
            ("prompt_drop.rs", include_str!("prompt_drop.rs")),
            ("transcript/rows.rs", include_str!("transcript/rows.rs")),
            ("transcript/scroll.rs", include_str!("transcript/scroll.rs")),
            ("scrollbar.rs", include_str!("scrollbar.rs")),
            ("file_links.rs", include_str!("file_links.rs")),
            ("main.rs", include_str!("main.rs")),
        ];
        let mut missing = Vec::new();
        for (file, source) in sources {
            let end = ["\n#[cfg(test)]\nmod ", "\n#[cfg(test)]\npub mod "]
                .iter()
                .filter_map(|module| source.find(module))
                .min()
                .unwrap_or(source.len());
            let body = &source[..end];
            for (at, line) in body.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                let code = code.split(" // ").next().unwrap_or(code);
                for c in code.chars().filter(|c| !c.is_ascii()) {
                    if !covers(GEIST_MONO, c) {
                        missing.push(format!(
                            "{file}:{} {c} (U+{:04X}) not in Geist Mono",
                            at + 1,
                            c as u32
                        ));
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "glyphs the face lacks:\n{}",
            missing.join("\n")
        );
    }
}
