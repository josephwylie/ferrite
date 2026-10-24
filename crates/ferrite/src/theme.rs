//! Ferrite's visual system: every colour, face, size and metric, named once.
//! This module doc is where the design rules live. Render code imports from
//! here and holds no colour or metric literal of its own; core stays
//! colour-blind. The picture these rules describe is the UI refresh's
//! (`docs/design/ui-refresh/`: `SPEC.md` is the build reference, its
//! `mockups/` the visual truth; ADR 0033 records the decisions).
//! `spikes/terminal-native/index.html` is no longer the approved picture.
//!
//! **A terminal in a card, calm chrome around it.** Ferrite keeps the
//! provider CLIs' terminal — a monospace transcript, `❯` prompts, `●` tool
//! bullets, `└` results, braille spinners, state inks — and keeps what only
//! a native app can do: vector marks, inline images, side-by-side diffs,
//! folds in place, pixel scrolling, floating pickers. Everything around the
//! transcript is quieter and modern: sans chrome, rounded cards on a
//! lighter frame, hairlines, and colour only for state. No chat bubbles, no
//! proportional transcript.
//!
//! The rules every render site follows:
//!
//! 1. **Two faces, one rule** (supersedes "one face, one grid"). The
//!    transcript is Geist Mono (`FONT_CODE`); everything else is Geist
//!    (`FONT_UI`): nav, heads, tabs, buttons, menus, the palette, the panel,
//!    the bar, the Composer's input. Numbers and glyphs from the terminal's
//!    world (diffstats, spinners, `❯`) stay in Geist Mono inside chrome.
//!    Chrome type is a set of roles (SPEC §3.5: `FS_UI` 13/20 default,
//!    `FS_TITLE`, `FS_LABEL`, `FS_META`, `FS_BADGE`, …) with tabular
//!    figures; the transcript is the reading size (`answer_text_size`,
//!    cmd-= / cmd-- / cmd-0, the transcript only) on its 1.6× line (a Group
//!    Pane's half a pixel smaller, `tx_size`), and
//!    everything inside it scales with it, measured in cells (`tx_cell`,
//!    the face's 0.6em). Chrome metrics are pixels on the space scale, not
//!    cells. Weights: 400 body; 500 tabs, buttons, Pane titles, labels;
//!    600 a view title, the prompt `❯`, a tool's name, strong prose,
//!    headings, a badge count. 700 is unused. Never reach a weight by
//!    family name.
//! 2. **Cards on a frame** (supersedes "square and flat inside"). The
//!    window's frame is `FRAME` #222 (the nav, the heads, the right panel,
//!    a Group's gutter); a Pane is a card on it, `CARD` #181818, radius
//!    `R_PANE`, a 1px `CARD_RING`. Only the bottom bar (`BAR`) is
//!    full-bleed. Radii run by role (`R_INLINE` 4 … `R_PALETTE` 12; dots,
//!    pills and the send circle round). Separation is a 1px white hairline
//!    (`HAIRLINE` 6% … `HAIRLINE_STRONG` 10%) or a fill step: no visible grey
//!    borders, no keycap boxes, no boxed section labels. Only a floating
//!    surface casts, one shadow (`components::float_shadow`); a modal adds
//!    `VEIL`.
//! 3. **Neutral greys.** The opaque surfaces, darkest first: `NODIFF` <
//!    `SUNKEN` (a command well) < `CARD` < `BAR` (the bar; diff and fence
//!    blocks) < `DECISION` < `FIELD_COMPACT` < `FRAME` = `FIELD` (the Solo
//!    Composer, the prompt band) < `TAB_ACTIVE` < `RAISED` (chips, inline
//!    code, secondary buttons, hover on the frame) < `SELECTED` (the
//!    selected row, tab, option, cursor) < `SELECTION_HOVER` (every press)
//!    < `SEND`. `FLOAT` grounds menus and popovers. `PRIMARY` (the
//!    strongest ink, with `ON_PRIMARY`) is the primary button.
//! 4. **Glass on the frame, terminal in the card** (macOS: `GLASS`). The
//!    window opens blurred (`window_background`) and its root paints nothing
//!    (`paint::WINDOW`); each region paints its own fill once, from
//!    `paint::*`, translucent on macOS: the frame and the bar at 88%, the
//!    card at 94% (a hint of the desktop, never a busy picture under text),
//!    fields, raised and selected fills as white overlays. gpui blurs no
//!    single element, so a float paints opaque (`paint::FLOAT`), and a
//!    surface that must occlude what scrolls under it (the sticky prompt, a
//!    Decision docked over the tail) stacks over its card, never over the
//!    bare window. Every other platform paints the opaque greys of rule 3.
//!    Fills and lines go through `theme::paint` (typed `Paint`, so `rgb()`
//!    cannot swallow one by mistake); the bare consts are the opaque values,
//!    for contrast math. **Never stack two region fills**: the frame and
//!    the card are separate regions, and what sits inside a region paints
//!    an overlay or nothing.
//! 5. **An ink ladder with floors**, brightest first: `TEXT_STRONG` #f2f2f2
//!    (titles, selected rows, the prompt, `**strong**`, `❯`, the caret,
//!    focus, links and paths in prose), `TEXT_PROSE` (assistant prose),
//!    `TEXT` (chrome text, nav titles, tool output), `TEXT_BAR` (emphasised
//!    bar items), `TEXT_MUTED` (unselected tabs and views, chips),
//!    `TEXT_DIM` #8c8c8c (metadata, times, icons, section headings, tool
//!    arguments; **the floor for readable text**: 4.5:1 on `FRAME` and
//!    `CARD`, 4:1 on `SELECTED`), `TEXT_FAINT` (structure only: `└`, rails,
//!    diff line numbers). **`TEXT_FAINT` is never text.** A row spends at
//!    most two text inks, one glyph ink and one colour (a diffstat's `+`/`−`
//!    pair may join a state's colour, ADR 0033).
//! 6. **No accent; colour is state** (supersedes "real terminal colour").
//!    Steel blue is retired: focus, selection, `❯`, the caret and links use
//!    `TEXT_STRONG`, and selection is a lighter fill, never a coloured
//!    outline. Hue is for state only: `RUNNING` green (working, `ok`, diff
//!    `+`), `ATTENTION` yellow (needs you, a risky permission mode,
//!    `mode_ink`), `BLOCKED` red (failed, diff `−`). Done is a shape
//!    (`check-circle`) in `TEXT_DIM`, not a hue. A provider's brand colour
//!    (`PROVIDER_CLAUDE`, `PROVIDER_CODEX`) is only on its logomark and its
//!    working glyphs. Syntax colour (`SYN_*`) is for code in diffs and fences
//!    only; inline code and paths are `TEXT_STRONG` (a path in a tool's
//!    arguments `TEXT_DIM`). A diff row washes red or green (`DIFF_*_WASH`,
//!    the changed words `DIFF_*_WORD`) under its code's own colours. The
//!    find wash (`FIND_WASH`) stays orange. **Colour sits on the word, the
//!    dot or the ring** — a blocked Pane's ring is `WAIT_RING`, a focused
//!    one's `FOCUS_RING` (white at 22%) — never on a ground.
//! 7. **One place per fact** (supersedes "clean": SPEC §5 places them). The
//!    bar is views and vitals; the Composer holds the mode, model and
//!    effort; the right panel holds changes, the pull request and context;
//!    a Group Pane's head speaks its state word. Nothing shows twice on one
//!    surface.
//! 8. **Little moves at once.** What moves: the working line's spinner
//!    (`components::working_spinner`, `· ✢ ✳ ✶ ✻ ✽` at
//!    `MOTION_WORKING_FRAME_MS`, the provider's colour) and its caption's
//!    shimmer (`components::shimmer`), on the focused Pane only; the braille
//!    spinner (`components::braille_spinner`, `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` at
//!    `MOTION_BRAILLE_FRAME_MS`, `RUNNING`) in place of the mark of every
//!    working Thread; the caret blinking in the focused Composer
//!    (`components::caret_blink`); the nav and the panel riding their width
//!    (`MOTION_RESIZE_MS`); hover fills blending in. **No dot pulses or
//!    breathes**; unread is ink (`TEXT_STRONG` title), not motion. The
//!    motion section below holds the rest of the catalog, and reduced
//!    motion holds every loop on a static end state that says the same
//!    thing.
//! 9. **Pixel line heights and a space scale.** Every text role is a (size,
//!    line height) pair, and fixed row heights are `const` expressions of
//!    those pairs, never hand-summed literals. Space: 2 · 4 · 6 · 8 · 12 ·
//!    16 · 24 · 32 (`SPACE_*`, named in gpui's 4px units); inside the
//!    transcript, its line and cells. A metric off the scale says why in its
//!    doc.
//! 10. **Glyph coverage.** Every glyph is text, set in its face where the
//!     face has it and in the face the platform falls back to where it does
//!     not — on macOS Menlo for `❯ ◆ ✓ ✗ ▾ ▸ ⌘ ⌥ ⌃` and the working
//!     spinner's stars, STIX Two Math for `⏵ ⏸`, Apple Braille for the
//!     braille spinner (`components::glyph`) — on the line's baseline.
//!     `CHROME_GLYPHS` lists the non-ASCII glyphs Geist Mono sets,
//!     `UI_GLYPHS` the ones the chrome sets in Geist (`⇧ ↑ ↓ · …`), and
//!     `FALLBACK_GLYPHS` the ones both lack; a test checks each against the
//!     faces. Icons are Ferrite's own line family (`icons.rs`, 1.5px at 16).
//! 11. **Words.** Ferrite's own copy speaks one shared word list
//!     (`theme::words`, beside the state inks and tested against the
//!     notifications, the Decision card and the transcript): `needs you`,
//!     `failing N`, `failed`, `interrupted`, `working`, `done`. Titles,
//!     buttons, menu items, labels, chip labels and empty states are
//!     sentence case (`New thread`, `Review PR`); state and value tokens are
//!     lowercase, even leading a row; Title Case lives only in the macOS menu
//!     bar. `·` is the only separator inside a line — no colon labels, no
//!     em dash, no final period on one-line copy — and no surface prints
//!     `now`. A shortcut is plain `TEXT_DIM` text, only where the operator
//!     is already looking (a menu, the palette, the `Search` row, a
//!     tooltip: `Toggle sidebar ⌘B`), read from the keymap, never typed by
//!     hand.
//!
//! **Layout of this file.** Everything down to `init_components` is the
//! shared head: values more than one surface reads, the paints, and the kit
//! mapping. Below it, one section per work package (`WP-A` … `WP-G`), each
//! opened by a banner and closed by an `(end WP-x)` line. A token two
//! sections need goes in the head.
//!
//! The contrast floors, the ladders, the hairlines, the radii, the paints,
//! the faces, the kit-token mapping and the derived row heights are
//! asserted in `theme::tests`. Dark only: operators work long sessions
//! beside dark editors and terminals.

use gpui::FontWeight;

// --------------------------------------------------------------- platform

/// Glass on the frame (rule 4): macOS opens the window blurred and paints
/// translucent fills; every other platform paints the opaque greys. The
/// `opaque` Cargo feature forces the opaque greys on macOS too (the
/// headless parity captures, which cannot show the desktop blur).
pub const GLASS: bool = cfg!(target_os = "macos") && !cfg!(feature = "opaque");

/// An opaque `0xRRGGBB` as a `0xRRGGBBAA` paint.
pub const fn solid(rgb: u32) -> u32 {
    (rgb << 8) | 0xff
}

/// A fill or a line whose value depends on the platform (rule 4):
/// `0xRRGGBBAA`, translucent glass on macOS and the opaque grey elsewhere.
/// It converts into `Hsla`, `Rgba` and `Fill`, so `.bg(paint::CARD)` and
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

// ---------------------------------------------------------------- surfaces
//
// The opaque greys (`docs/design/ui-refresh/SPEC.md` §3.1), darkest first,
// each beside its glass value (`0xRRGGBBAA`, rule 4). Paint a region with
// `paint::*`, which picks between them; the bare consts are the opaque
// values, for contrast math and the few places that must be opaque.

/// `#161616` — sunk below the card: a Decision's command well, the
/// palette scrim's base.
pub const SUNKEN: u32 = 0x161616;
/// Sunk as glass: a black overlay on the card.
pub const SUNKEN_GLASS: u32 = 0x00000014;
/// `#181818` — the card: a Pane's body, the transcript's surface.
pub const CARD: u32 = 0x181818;
/// The card as glass: 94%, a terminal's background opacity.
pub const CARD_GLASS: u32 = 0x181818f0;
/// `#1e1e1e` — the bottom bar; diff and code-fence blocks.
pub const BAR: u32 = 0x1e1e1e;
pub const BAR_GLASS: u32 = 0x1e1e1ee0;
/// `#1f1f1f` — the Decision box.
pub const DECISION: u32 = 0x1f1f1f;
/// The Decision box as glass: a white overlay on the card.
pub const DECISION_GLASS: u32 = 0xffffff08;
/// `#202020` — the compact Composer in a Group Pane.
pub const FIELD_COMPACT: u32 = 0x202020;
/// The compact Composer as glass: a white overlay on the card.
pub const FIELD_COMPACT_GLASS: u32 = 0xffffff09;
/// `#222222` — the frame behind the cards: the nav, the heads, the right
/// panel, a Group's gutter.
pub const FRAME: u32 = 0x222222;
/// The frame as glass, 88%. The window's blur shows whatever sits behind
/// Ferrite — other windows included — so a bright window behind turned a
/// 72% sidebar light grey and its dim ink fell to 2.8:1. At 88% the frame
/// stays dark over pure white and still takes a tint of what is behind.
pub const FRAME_GLASS: u32 = 0x222222e0;
/// `#222222` — a field on the card: the Solo Composer's box, the prompt
/// band. The frame's value, a step over the card.
pub const FIELD: u32 = 0x222222;
/// A field as glass: a white overlay on the card.
pub const FIELD_GLASS: u32 = 0xffffff0d;
/// `#262626` — the active tab inside a card.
pub const TAB_ACTIVE: u32 = 0x262626;
/// The active tab as glass: a white overlay on the card.
pub const TAB_ACTIVE_GLASS: u32 = 0xffffff0f;
/// `#2a2a2a` — raised a step: chips, inline code, secondary buttons, the
/// panel's file block, and the hover fill on the frame.
pub const RAISED: u32 = 0x2a2a2a;
pub const RAISED_GLASS: u32 = 0xffffff0f;
/// `#2a2a2c` — the floating ground: menus, pickers, the palette, popovers,
/// tooltips, toasts, sheets.
pub const FLOAT: u32 = 0x2a2a2c;
/// A float as glass: opaque. gpui blurs no single element, so a backdrop
/// the prototype's blur softened would let the rows under a menu ghost
/// through as sharp text; a float must stay legible on its own.
pub const FLOAT_GLASS: u32 = 0x2c2c2fff;
/// `#2e2e30` · `#343437` — a float's hover and cursor fills, opaque steps
/// over its own ground: `RAISED` and `SELECTED` are tuned to the frame
/// (#222) and land 2 under and 4 over `FLOAT`, so on a float the pointer
/// showed nothing and the cursor next to nothing. The cursor is the clear
/// step (+10); the hover stays faint enough (+4) to keep `TEXT_DIM` on its
/// floor, and the cursor row lifts its dim words to `TEXT_MUTED`
/// (`menu::row_face`), as the nav's selected row lifts its line 2.
pub const FLOAT_RAISED: u32 = 0x2e2e30;
pub const FLOAT_SELECTED: u32 = 0x343437;
/// `#2e2e2e` — selected: the selected nav row, the active view and panel
/// tab, the cursor row of a menu, a Decision option and the palette.
pub const SELECTED: u32 = 0x2e2e2e;
pub const SELECTED_GLASS: u32 = 0xffffff1c;
/// `#363636` — a selected row under the pointer, and every press.
pub const SELECTION_HOVER: u32 = 0x363636;
pub const SELECTION_HOVER_GLASS: u32 = 0xffffff24;
/// `#3a3a3a` — the send button's circle (and a comment's avatar).
pub const SEND: u32 = 0x3a3a3a;
/// `#141414` — the empty side of a split diff (a row with no partner),
/// below the card.
pub const NODIFF: u32 = 0x141414;
pub const NODIFF_GLASS: u32 = 0x00000033;
/// `#f2f2f2` / `#161616` — the primary button (`primary_button`, dialogs
/// and settings) and its ink. No hue: the primary is the strongest ink.
pub const PRIMARY: u32 = TEXT_STRONG;
pub const ON_PRIMARY: u32 = 0x161616;

// -------------------------------------------------------------- hairlines
//
// White at an opacity over whatever is below (SPEC §3.2): the same on
// glass and on the opaque greys, so these are `0xRRGGBBAA` and paint with
// `rgba()` or `paint::*`.

/// 1px — every hairline's width (rule 2).
pub const HAIRLINE_W: f32 = 1.0;
/// 6% — dividers in cards and panels: a tab strip's foot, the queued row's
/// foot, panel sections.
pub const HAIRLINE: u32 = 0xffffff0f;
/// 7% — the 1px ring around a card.
pub const CARD_RING: u32 = 0xffffff12;
/// 8% — the 1px ring around the Composer.
pub const FIELD_RING: u32 = 0xffffff14;
/// 10% — a float's edge.
pub const HAIRLINE_STRONG: u32 = 0xffffff1a;
/// 6% — the inset ring on the selected nav row.
pub const ROW_RING: u32 = 0xffffff0f;
/// 5% — the bottom bar's top edge.
pub const BAR_TOP: u32 = 0xffffff0d;
/// 22% — the focused Pane on a Group (more than one Pane shows), a drop
/// target's edge.
pub const FOCUS_RING: u32 = 0xffffff38;
/// `ATTENTION` at 35% — the ring of a Pane whose Thread is blocked,
/// focused or not.
pub const WAIT_RING: u32 = 0xe6c47c59;
/// `ATTENTION` at 22% — the Decision box's inset ring.
pub const DECISION_RING: u32 = 0xe6c47c38;
/// 6% — the compact Composer's ring at rest; 12% on the focused Pane.
pub const COMPACT_RING: u32 = 0xffffff0f;
pub const COMPACT_RING_FOCUSED: u32 = 0xffffff1f;
/// `BLOCKED` at 7% — the failure strip's fill.
pub const FAILURE_WASH: u32 = 0xef8a8012;

// ------------------------------------------------------------ rule greys
//
// Two opaque rules, for what a hairline cannot carry: a seam, a table's
// rules, a code block's rail, an image's frame, a board's lines. Chrome
// separates by the hairlines above.

/// `#2c2c2c` — a rule: a seam, a table's row rules, a break.
pub const LINE: u32 = 0x2c2c2c;
pub const LINE_GLASS: u32 = 0xffffff14;
/// `#3a3a3a` — a strong rule: a table's head rule, a code block's
/// rail, an image's frame.
pub const LINE2: u32 = 0x3a3a3a;
pub const LINE2_GLASS: u32 = 0xffffff24;
/// Fully transparent — an edge that is always in layout and only changes
/// colour, so nothing reflows when a Decision or a blocker arrives.
pub const TRANSPARENT: u32 = 0x00000000;

/// **The paints** (rule 4): what a region lays down, glass on macOS and
/// opaque grey elsewhere. Every fill and line is painted from here.
pub mod paint {
    use super::{solid, Paint, TRANSPARENT};

    /// The window's own root: nothing on glass, so each region's fill shows
    /// the blur behind the window; the card elsewhere.
    pub const WINDOW: Paint = Paint::pick(TRANSPARENT, solid(super::CARD));
    pub const CARD: Paint = Paint::pick(super::CARD_GLASS, solid(super::CARD));
    pub const FRAME: Paint = Paint::pick(super::FRAME_GLASS, solid(super::FRAME));
    pub const BAR: Paint = Paint::pick(super::BAR_GLASS, solid(super::BAR));
    pub const FIELD: Paint = Paint::pick(super::FIELD_GLASS, solid(super::FIELD));
    pub const RAISED: Paint = Paint::pick(super::RAISED_GLASS, solid(super::RAISED));
    pub const SELECTED: Paint = Paint::pick(super::SELECTED_GLASS, solid(super::SELECTED));
    pub const SUNKEN: Paint = Paint::pick(super::SUNKEN_GLASS, solid(super::SUNKEN));
    pub const DECISION: Paint = Paint::pick(super::DECISION_GLASS, solid(super::DECISION));
    pub const FIELD_COMPACT: Paint =
        Paint::pick(super::FIELD_COMPACT_GLASS, solid(super::FIELD_COMPACT));
    pub const TAB_ACTIVE: Paint = Paint::pick(super::TAB_ACTIVE_GLASS, solid(super::TAB_ACTIVE));
    pub const SELECTION_HOVER: Paint =
        Paint::pick(super::SELECTION_HOVER_GLASS, solid(super::SELECTION_HOVER));
    /// Every press: the selected row's hover face.
    pub const PRESS: Paint = SELECTION_HOVER;
    /// The send circle: a control, not a region, so opaque everywhere (a
    /// PR comment's avatar wears it too).
    pub const SEND: Paint = Paint(solid(super::SEND));
    /// An open pull request's pill: a hue overlay on every platform.
    pub const PR_OPEN_PILL: Paint = Paint(super::PR_OPEN_PILL);
    pub const FLOAT: Paint = Paint::pick(super::FLOAT_GLASS, solid(super::FLOAT));
    pub const NODIFF: Paint = Paint::pick(super::NODIFF_GLASS, solid(super::NODIFF));
    /// The hairlines: white overlays on every platform.
    pub const HAIRLINE: Paint = Paint(super::HAIRLINE);
    pub const HAIRLINE_STRONG: Paint = Paint(super::HAIRLINE_STRONG);
    pub const BAR_TOP: Paint = Paint(super::BAR_TOP);
    pub const LINE: Paint = Paint::pick(super::LINE_GLASS, solid(super::LINE));
    pub const LINE2: Paint = Paint::pick(super::LINE2_GLASS, solid(super::LINE2));

    /// The prompt band: a field that must occlude what scrolls under it, so
    /// on glass it is 80% over the card (composing to ~99%), not an overlay.
    pub const PROMPT_BAND: Paint = Paint::pick(0x222222cc, solid(super::FIELD));
}

// ------------------------------------------------------------------ edges

/// `#7c7c7c` — a resting checkbox, radio or switch boundary: solid and at
/// least 3:1 on the card, the frame, a float and a field, so an unchecked
/// control never vanishes.
pub const INPUT_EDGE: u32 = 0x7c7c7c;
/// `#3a3a3a` / `#555555` — the scrollbar thumb, at rest and under the
/// pointer.
pub const SCROLLBAR: u32 = 0x3a3a3a;
pub const SCROLLBAR_HOVER: u32 = 0x555555;
/// `#ffffff1a` — an unlit meter segment and the usage lines' tracks.
pub const METER_OFF: u32 = 0xffffff1a;
/// The veil behind a modal sheet: lighter over glass, which already dims.
pub const VEIL: u32 = if GLASS { 0x00000033 } else { 0x00000059 };

// -------------------------------------------------------------------- ink

/// `#f2f2f2` — titles, selected rows, the prompt text, `**strong**`, `❯`,
/// the caret, focus, links and paths in prose, inline code.
pub const TEXT_STRONG: u32 = 0xf2f2f2;
/// `#e6e6e6` — assistant prose.
pub const TEXT_PROSE: u32 = 0xe6e6e6;
/// `#d0d0d0` — default chrome text, nav titles, tool output.
pub const TEXT: u32 = 0xd0d0d0;
/// `#c0c0c0` — emphasised bar items.
pub const TEXT_BAR: u32 = 0xc0c0c0;
/// `#a0a0a0` — unselected tabs and nav views, chips.
pub const TEXT_MUTED: u32 = 0xa0a0a0;
/// `#8c8c8c` — metadata: times, icons, section headings, tool arguments
/// and results, hints, placeholders. **The floor for readable text**:
/// 4.5:1 on `FRAME` and `CARD`, 4:1 on `SELECTED`.
pub const TEXT_DIM: u32 = 0x8c8c8c;
/// `#5e5e5e` — structure, never words: `└`, diff line numbers, rails,
/// disclosure glyphs, the parked ring.
pub const TEXT_FAINT: u32 = 0x5e5e5e;
/// The dim ink on the frame's glass (the sidebar, the bottom bar). Over a
/// see-through ground the opaque `TEXT_DIM` loses its contrast against a
/// bright desktop, so on glass it lifts a step; opaque builds keep it.
pub const CHROME_MUTED: u32 = if GLASS { 0xb6b6b6 } else { TEXT_DIM };

// --------------------------------------------------------------- palette
//
// The named hues: state, syntax, the context legend. Semantic tokens below
// point at them; render code names the semantic token.

pub const RED: u32 = 0xef8a80;
pub const GREEN: u32 = 0x93cf8c;
pub const YELLOW: u32 = 0xe6c47c;
pub const BLUE: u32 = 0x82b1f2;
pub const MAGENTA: u32 = 0xc59df0;
pub const CYAN: u32 = 0x78ccd0;
pub const ORANGE: u32 = 0xeda879;

// ----------------------------------------------------- focus and selection

/// Keyboard focus on a control (`components::control_focus`, the kit's
/// `ring`): the strongest ink, never a hue (rule 2).
pub const CONTROL_FOCUS: u32 = TEXT_STRONG;
/// White at 20% — native text selection, painted over glyphs.
pub const TEXT_SELECTION_WASH: u32 = 0xffffff33;
/// `#eda87933` (20%) — every occurrence a find (⌘F) marks, under its
/// glyphs: orange, a hue no state wears, kept apart from the white
/// selection and from yellow, which means "needs you".
pub const FIND_WASH: u32 = 0xeda87933;
/// `#eda87980` (50%) — the occurrence the find is on.
pub const FIND_ACTIVE_WASH: u32 = 0xeda87980;
/// The caret in the focused Composer.
pub const CARET: u32 = TEXT_STRONG;
/// A board seam under the pointer or held: `TEXT_STRONG` at 40%.
pub const SEAM_HOVER: u32 = 0xf2f2f266;

// -------------------------------------------------------- state + signals

/// Green — live work: the running dot and braille spinner, `ok`, a passing
/// check, diff `+`.
pub const RUNNING: u32 = GREEN;
/// Yellow — needs you: the dot, the `◆`, the kind word, a risky mode.
pub const ATTENTION: u32 = YELLOW;
/// Red — failed: the dot, the word `failed`, diff `−`.
pub const BLOCKED: u32 = RED;
/// Red as a one-line ground (a refused drop, a destructive control under
/// the pointer); never a multi-line wash (`DIFF_REMOVED_WASH`).
pub const BLOCKED_WASH: u32 = 0xef8a801f;
/// The idle/parked dot: the dim ink in a dot role. (Done is a shape, the
/// check-circle, in this ink too: no hue, ADR 0033.)
pub const IDLE: u32 = TEXT_DIM;

/// A permission mode's ink, by its id: `ATTENTION` for one that lets
/// everything run (Claude's bypass, Codex's full access), else
/// `TEXT_MUTED` — the mode chip's (SPEC §5.5).
pub fn mode_ink(mode: &str) -> u32 {
    match mode {
        "bypassPermissions" | "danger-full-access" => ATTENTION,
        _ => TEXT_MUTED,
    }
}

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
    /// `NEEDS_YOU` counted past one (`2 need you`).
    pub const NEED_YOU: &str = "need you";
    /// What a waiting agent needs: a permission request.
    pub const APPROVAL: &str = "approval";
    /// What a waiting agent needs: an answer.
    pub const QUESTION: &str = "question";
    /// A turn finished cleanly.
    pub const DONE: &str = "done";
    /// A turn or an agent failed (`BLOCKED`).
    pub const FAILED: &str = ferrite_core::digest::FAILED;
    /// Tests are failing (`BLOCKED`).
    pub const FAILING: &str = "failing";
    /// The operator (or the runtime) stopped the work.
    pub const INTERRUPTED: &str = "interrupted";
    /// Live work.
    pub const WORKING: &str = "working";
    /// Main is idle while its Subagents or background tasks still work.
    pub const BACKGROUND: &str = "background";
    /// What a waiting agent needs: its plan approved.
    pub const PLAN: &str = "plan";
    /// Not working and nothing unseen.
    pub const IDLE: &str = "idle";
    /// Waiting on the operator: the status filter's name for it.
    pub const BLOCKED: &str = "blocked";
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
    /// A machine's Server is being reached, or reached again (ADR 0026).
    pub const CONNECTING: &str = "connecting";
    /// A machine the operator turned off in Settings.
    pub const OFF: &str = "off";
}

/// A machine whose Server went (ADR 0026): what it last showed stays, at
/// this opacity, and takes no input until it is back.
pub const MACHINE_LOST_OPACITY: f32 = 0.5;

/// The ink a lexicon word wears when it leads a line: `needs you` is
/// `ATTENTION`, `failed`/`failing` are `BLOCKED`, every other word is
/// `TEXT_DIM` — colour always means something needs you.
pub fn word_ink(word: &str) -> u32 {
    match word {
        words::NEEDS_YOU => ATTENTION,
        words::FAILED | words::FAILING => BLOCKED,
        _ => TEXT_DIM,
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

/// A permission mode's word (the mode chip's label, sentence-cased, when
/// the Session lists none), or `None` at the default (empty or `default`).
/// An unknown id is split at its camelCase humps and lowercased (`dontAsk`
/// → `dont ask`), so no raw id ever shows.
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
/// Comments are read, not decoration: `TEXT_DIM`, set italic.
pub const SYN_COMMENT: u32 = TEXT_DIM;
/// Punctuation: the plain ink (the prototype leaves `::`, `(`, `;` unclassed).
pub const SYN_PUNCT: u32 = TEXT;
/// Everything the highlighter leaves unclassed.
pub const SYN_PLAIN: u32 = TEXT;
/// A link or a path in prose, a file chip's name: `TEXT_STRONG`, underlined
/// under the pointer (cyan is retired, ADR 0033). A path in a tool call's
/// arguments is `TEXT_DIM` with the rest of them.
pub const LINK_INK: u32 = TEXT_STRONG;
/// The wash over the slot a dragged Pane would take: white at 6%.
pub const DROP_WASH: u32 = 0xffffff0f;
/// That slot's edge: the focus ring's white at 22%.
pub const DROP_EDGE: u32 = FOCUS_RING;
/// A diff row's changed words: the row's hue at 24% (added) and 28%
/// (removed), over the row's own wash (`DIFF_ADDED_WASH`,
/// `DIFF_REMOVED_WASH`).
pub const DIFF_ADDED_WORD: u32 = 0x93cf8c3d;
pub const DIFF_REMOVED_WORD: u32 = 0xef8a8047;
/// The alphas the changed words' washes paint with: the prototype's 24% and
/// 28%, as gpui's blend must take them to land on the browser's 8-bit ink
/// over the row's wash (`#41533f` added, `#664240` removed, sampled from
/// its shots) — the two compositors round the same fraction differently.
pub const DIFF_ADDED_WORD_ALPHA: f32 = 0.237;
pub const DIFF_REMOVED_WORD_ALPHA: f32 = 0.2765;
/// A diff row's sign: the only hued glyph on the row.
pub const DIFF_ADDED_SIGN: u32 = GREEN;
pub const DIFF_REMOVED_SIGN: u32 = RED;
/// How far the working caption's shimmer lifts its colour toward white at
/// the crest (`components::shimmer`): Claude's clay crests near `#ffe1d3`.
pub const SHIMMER_LIFT: f32 = 0.75;
/// Claude's shimmer crest: the prototype's gradient's middle stop.
pub const SHIMMER_CREST_CLAUDE: u32 = 0xffe1d3;

// ------------------------------------------------------------- the float
//
// **One cast shadow** (rule 2): a floating surface's, CSS `0 6px 14px` at
// 50%. gpui's blur is a gaussian σ, half a CSS blur. Nothing else casts
// and nothing is lit. The palette's dialog (and the shortcuts sheet, which
// wears it) deepens it to `SHADOW_PALETTE_*`: SPEC §5.8 gives that depth to
// the palette alone, and every other float takes the palette's shell "at a
// smaller scale", its shadow included.

pub const SHADOW_FLOAT: u32 = 0x00000080;
pub const SHADOW_FLOAT_Y: f32 = 6.0;
pub const SHADOW_FLOAT_BLUR: f32 = 7.0;
/// The dialog's shadow: the float's deepened to CSS `0 24px 64px` at 55%.
pub const SHADOW_PALETTE: u32 = 0x0000008c;
pub const SHADOW_PALETTE_Y: f32 = 24.0;
pub const SHADOW_PALETTE_BLUR: f32 = 32.0;

// ------------------------------------------------------------------- type
//
// **Two faces** (rule 1). The chrome is Geist (`FONT_UI`) at `FS_UI` 13 on
// `LH_UI` 20 by default, with the roles of SPEC §3.5 below (`FS_TITLE`,
// `FS_LABEL`, `FS_META`, …). The transcript is Geist Mono (`FONT_CODE`) at
// the reading size on its own 1.6× line (`answer_text_size`,
// `answer_line_height`; 13 on 21 at Standard, half a pixel smaller in a
// Group Pane: `tx_size`) and every transcript measure
// scales from it. Numbers and glyphs from the terminal's world (diffstats,
// spinners, `❯`) stay in Geist Mono inside chrome too.
//
// **Weights:** `W_BODY` (400) for everything that is read; `W_LABEL` (500)
// tabs, buttons, Pane titles, labels; `W_STRONG` (600) a view title, the
// prompt `❯`, a tool's name, headings, `**strong**`, a badge count. 700
// is never used, and no render site names a `FontWeight` itself. Weight
// never signals state on a row that can truncate.
//
// **Figures:** the chrome sets tabular figures (`components::tabular` on
// the window root), so a ticking count or a column of numbers never
// shifts. Geist Mono's figures are tabular already, and it ships no
// ligatures (`theme::tests`).

/// 13px — the grid's size: every chrome line, on `LH_UI`.
pub const FS_UI: f32 = 13.0;
/// 20px — the grid's row.
pub const LH_UI: f32 = 20.0;
/// 13px — the Standard reading size (`ReadingSize::STANDARD`): agent prose
/// and every Solo transcript row at the default zoom, on `LH_PROSE`.
pub const FS_PROSE: f32 = 13.0;
/// 21px — the Standard reading size's line: 1.6×, whole (SPEC §3.5's
/// 13/21; `prose_line_height`).
pub const LH_PROSE: f32 = 21.0;
/// 13.5 on 20 — a view title (the Solo and Group heads), at `W_STRONG`.
pub const FS_TITLE: f32 = 13.5;
/// 12.5 — tabs, buttons, Pane titles, panel tabs, at `W_LABEL` on `LH_UI`.
pub const FS_LABEL: f32 = 12.5;
/// 12 on 16 (or `LH_UI`) — metadata, section headings, the bar, a row's
/// second line.
pub const FS_META: f32 = 12.0;
pub const LH_META: f32 = 16.0;
/// 11 — a badge's count, at `W_STRONG`.
pub const FS_BADGE: f32 = 11.0;
/// 13.5 on 20 — the Solo Composer's input (a Group's is `FS_UI`).
pub const FS_COMPOSER: f32 = 13.5;
/// 14.5 — the palette's query.
pub const FS_PALETTE: f32 = 14.5;
/// 11 on 19 — the panel's diff, in Geist Mono.
pub const FS_PANEL_DIFF: f32 = 11.0;
pub const LH_PANEL_DIFF: f32 = 19.0;
/// 11.5 — a diffstat, a spinner or a `#N` set in Geist Mono inside chrome.
pub const FS_CHROME_CODE: f32 = 11.5;

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

/// The pixel line height paired with each reading size: 1.6× and whole
/// (`prose_line_height`), the terminal's row: 12/19 · 13/21 · 14/22 ·
/// 15/24 · 16/26 · 18/29 · 20/32 · 22/35 · 24/38.
pub fn answer_line_height(size: ferrite_core::settings::ReadingSize) -> f32 {
    prose_line_height(answer_text_size(size))
}

/// How much smaller a Group Pane's transcript is set than the reading
/// size: one half-pixel step (SPEC §3.5's 12.5/20 at Standard), so four
/// Panes hold more of their Threads.
pub const TX_GROUP_STEP: f32 = 0.5;
/// How much smaller a diff or a code fence is set than the transcript
/// around it (SPEC §3.5: 12.5/20 in a Solo transcript at Standard).
pub const TX_BLOCK_STEP: f32 = 0.5;

/// A transcript's text size: the reading size in Solo, `TX_GROUP_STEP`
/// under it in a Group Pane.
pub fn tx_size(size: ferrite_core::settings::ReadingSize, solo: bool) -> f32 {
    answer_text_size(size) - if solo { 0. } else { TX_GROUP_STEP }
}

/// Headings are the body size (rule 1): they stand apart by weight and ink.
/// Kept as a function so the Markdown path has one place to ask.
pub fn heading_scale(_level: u8) -> f32 {
    1.
}

/// The transcript's line over its size: the mockups' 13/21 and 12.5/20
/// (SPEC §3.5).
pub const TX_LINE_RATIO: f32 = 1.6;

/// A transcript size's pixel line height: `round(size × 1.6)`.
pub fn prose_line_height(size: f32) -> f32 {
    (size * TX_LINE_RATIO).round()
}

/// 0.6em — Geist Mono's advance width (600/1000 em): every glyph's cell.
pub const CODE_ADVANCE: f32 = 0.6;
/// 7.8px — one cell at `FS_UI`.
pub const CODE_CELL: f32 = FS_UI * CODE_ADVANCE;
/// One cell at the grid size: every horizontal metric on the grid is a
/// whole number of these (the 2-cell glyph gutter, a 36-cell sidebar).
pub const CH: f32 = CODE_CELL;
/// One row of the grid, and half of one: every vertical metric on the grid
/// is a whole number of half rows.
pub const ROW: f32 = LH_UI;
pub const HALF_ROW: f32 = LH_UI / 2.0;
/// The 2-cell glyph gutter the prototype hangs every mark in (`❯`, `●`,
/// `└`, the spinner) at the grid size; a transcript's gutter is two cells
/// at its own reading size.
pub const GLYPH_GUTTER: f32 = 2.0 * CH;
/// 108 cells — the prose measure: the most a paragraph, a code block or a
/// table runs before it wraps, at any reading size.
pub const MEASURE_CH: f32 = 108.0;
/// A floor under the face's advance for laying out against text width
/// estimates (a title's floor). With one monospace face every advance is
/// `CODE_ADVANCE`; this stays a floor so a short label is never padded.
pub const UI_ADVANCE_FLOOR: f32 = 0.5;

/// The non-ASCII glyphs Geist Mono sets: every one is in its cmap (asserted
/// by `theme::tests`). A rule the tests enforce, so it compiles only with
/// them.
#[cfg(test)]
pub const CHROME_GLYPHS: &[char] = &[
    '●', '○', '└', '├', '│', '─', '·', '•', '…', '↑', '↓', '←', '→', '↳', '↩', '⇧', '⇥', '⏎', '↵',
    '⌫', '±', '−', '—', '×', '›', '‹', '▲', '▼', '▶', '◀',
];
/// The non-ASCII glyphs the chrome sets in Geist (`FONT_UI`): every one is in
/// its cmap (asserted by `theme::tests`). `⌘ ⌥ ⌃` are the platform's in both
/// faces (`FALLBACK_GLYPHS`).
#[cfg(test)]
pub const UI_GLYPHS: &[char] = &['⇧', '↑', '↓', '·', '⇥', '…', '—', '−', '×', '●', '○'];
/// The glyphs the design uses that Geist Mono lacks: each is typed all the
/// same, and the platform's fallback face sets it (rule 10). Asserted
/// *absent* from the face, so a face update that adds one is noticed.
#[cfg(test)]
pub const FALLBACK_GLYPHS: &[char] = &[
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

// ------------------------------------------------------------------ radii
//
// **Radii by role** (rule 2, SPEC §3.6): 4 · 5 · 6 · 7 · 8 · 10 · 12, and
// round for dots, pills and the send circle (`rounded_full`).

/// 4px — the inline code chip.
pub const R_INLINE: f32 = 4.0;
/// 5px — a view tab in the bar, a 22px icon button.
pub const R_SMALL: f32 = 5.0;
/// 6px — a 28px icon button, a tab in a card, a Decision option, the
/// command well.
pub const R_TAB: f32 = 6.0;
/// 7px — controls: nav rows, buttons, fields, the prompt band, diff and
/// fence blocks. The kit's `radius`.
pub const R_CONTROL: f32 = 7.0;
/// 7px — chips (and a menu's rows).
pub const R_CHIP: f32 = R_CONTROL;
/// 8px — blocks: floats, the Group Composer, the Decision box, the failure
/// strip, the panel's file block. The kit's `radius_lg`.
pub const R_BLOCK: f32 = 8.0;
/// 10px — the Pane card, the Solo Composer.
pub const R_PANE: f32 = 10.0;
/// 12px — the palette.
pub const R_PALETTE: f32 = 12.0;
/// 2px — meter segments and other tiny marks.
pub const R_TIGHT: f32 = 2.0;

// --------------------------------------------------------- shell and board

/// 264px — the navigation column (SPEC §5.1); folded it is the rail.
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input. Off the space scale: the mockups' column.
pub const NAV_WIDTH: f32 = 264.0;
/// 46px — the head row (SPEC §4), ADR 0014's titlebar grown into the row
/// that holds the traffic lights over the nav, the view head over the board
/// and the panel's tabs over the panel. The host traffic lights centre here
/// (`TRAFFIC_Y`), the row drags the window outside its controls, and the
/// board's cards start below it (`titlebar::board_top`). Off the space
/// scale: a 28px control and 9px of frame either side.
pub const WIN_CHROME_H: f32 = TITLEBAR_H;
/// 46px — the head row, and `STATUS_BAR_H` 30 the bottom bar (full width,
/// `BAR` under a `BAR_TOP` line). Off the scale: the mockups' rows.
pub const TITLEBAR_H: f32 = 46.0;
pub const STATUS_BAR_H: f32 = 30.0;
/// The view head in the head row over the board (SPEC §5.2): padded
/// `VIEW_HEAD_PAD_L` (6) inside the board's column, its parts
/// `VIEW_HEAD_GAP` (10) apart — the title (`FS_TITLE` 600), the Project in
/// `TEXT_DIM`, a spacer, then `New thread` (Group) and, while the panel is
/// hidden, the panel toggle. A secondary button is `CONTROL_H` at
/// `R_CONTROL` on `RAISED`, padded `CONTROL_CHIP_PAD_X` (10), its icon
/// and label `CONTROL_CHIP_GAP` (6) apart, the icon `CONTROL_CHIP_ICON`
/// (13). Off the scale: the mockups' head.
pub const VIEW_HEAD_PAD_L: f32 = SPACE_1_5;
pub const VIEW_HEAD_GAP: f32 = 10.0;
pub const CONTROL_CHIP_PAD_X: f32 = 10.0;
pub const CONTROL_CHIP_GAP: f32 = SPACE_1_5;
pub const CONTROL_CHIP_ICON: f32 = 13.0;
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
/// 8px — the frame the board column leaves right of and under its cards
/// (SPEC §4): the same gutter as between two cards (`GRID_GAP`). The left
/// has none: the nav's own padding is the gutter there.
pub const BOARD_PAD: f32 = SPACE_2;

// ----------------------------------------------------------------- panel

/// 304px — the right panel (SPEC §5.6), a fixed column on the
/// frame outside the board; padding 0 10 8 8 (`PANEL_PAD_*`, the right a
/// step wider than the gutter, as the nav's). Off the space scale: the
/// mockups' column.
pub const PANEL_W: f32 = 304.0;
pub const PANEL_PAD_L: f32 = SPACE_2;
pub const PANEL_PAD_R: f32 = 10.0;
pub const PANEL_PAD_B: f32 = SPACE_2;
/// The panel body's scrollbar lies in the column's right padding
/// (`PANEL_PAD_R`), beside the rows rather than over them, as the nav's
/// does: a 10px hit strip, the 6px hovered thumb 2px from either side.
pub const PANEL_SCROLLBAR_INSET: f32 = 2.0;
const _: () = assert!(PANEL_PAD_R == 2. * PANEL_SCROLLBAR_INSET + SCROLLBAR_THUMB_W_HOVER);
/// 28px — a panel tab in the head row (r7, 12.5/500), 10px inline padding,
/// 6px to its count; 2px between tabs.
pub const PANEL_TAB_H: f32 = 28.0;
pub const PANEL_TAB_PAD_X: f32 = 10.0;
/// 48px — a two-line panel row (the branch row, a file block's head): its
/// 14px icon at x 10, y 9; its text at x 34 (SPEC §4's panel rows).
pub const PANEL_ROW_H: f32 = 48.0;
pub const PANEL_ROW_ICON: f32 = 14.0;
pub const PANEL_ROW_ICON_X: f32 = 10.0;
pub const PANEL_ROW_ICON_Y: f32 = 9.0;
pub const PANEL_ROW_TEXT_X: f32 = 34.0;
/// 30px — a panel section line (`N files changed`, diffstat right), 8px
/// below the row over it.
pub const PANEL_SECTION_H: f32 = 30.0;
/// 32px — the panel's full-width secondary buttons (`Review changes`).
pub const PANEL_BUTTON_H: f32 = 32.0;
/// A file block's hunk area (phase 7b): on `BAR`, padding 4 0 6, its lines
/// in the panel's diff face (`FS_PANEL_DIFF`/`LH_PANEL_DIFF`), each on a
/// 2.5-cell sign column inset 1 cell; no line numbers.
pub const PANEL_HUNK_PAD_T: f32 = SPACE_1;
pub const PANEL_HUNK_PAD_B: f32 = SPACE_1_5;
pub const PANEL_HUNK_INSET_CELLS: f32 = 1.0;
pub const PANEL_HUNK_SIGN_CELLS: f32 = 2.5;
/// The foot's context meter: 6px high, 2px between segments, radius 2,
/// over an empty track of `PANEL_METER_TRACK`; the foot's padding is
/// 14 10 4 with 10px between its line and the meter (off the scale: the
/// mockup's foot).
pub const PANEL_METER_H: f32 = 6.0;
pub const PANEL_METER_TRACK: u32 = 0x333333;
pub const PANEL_FOOT_PAD_T: f32 = 14.0;
pub const PANEL_FOOT_GAP: f32 = 10.0;
pub const PANEL_FOOT_PAD_B: f32 = SPACE_1;

// ------------------------------------------------------------ PR tab

//
// Off the space scale where the mockup's PR tab is (`pr-states.html`): its
// 10px row gaps and paddings, the pill's 5 · 7 · 9, the empty state's 18
// and 36.

/// 14px — between the PR tab's sections (SPEC §5.6.2), and under its head.
pub const PR_SECTION_GAP: f32 = 14.0;
/// The state pill: 22 high, round (r11), padding 0 9 0 7, a 13px icon 5px
/// from its word.
pub const PR_PILL_H: f32 = 22.0;
pub const PR_PILL_PAD_L: f32 = 7.0;
pub const PR_PILL_PAD_R: f32 = 9.0;
pub const PR_PILL_GAP: f32 = 5.0;
pub const PR_PILL_ICON: f32 = 13.0;
/// `RUNNING` at 14% — an open pull request's pill (`paint::PR_OPEN_PILL`).
pub const PR_OPEN_PILL: u32 = 0x93cf8c24;
/// The pull request's title: 14/20 600, at most three lines, 10px under
/// the head's line.
pub const FS_PR_TITLE: f32 = 14.0;
pub const PR_TITLE_GAP: f32 = 10.0;
/// 10px — a check or readiness row's mark to its text.
pub const PR_ROW_GAP: f32 = 10.0;
/// 10px — a comment card's block padding (12px inline, `SPACE_3`).
pub const PR_COMMENT_PAD_Y: f32 = 10.0;
/// The empty state (no pull request, no `gh`): 18px down, its parts 10px
/// apart; a 36px badge (r10, `RAISED`) holding a 17px icon.
pub const PR_EMPTY_PAD_T: f32 = 18.0;
pub const PR_EMPTY_GAP: f32 = 10.0;
pub const PR_EMPTY_BADGE: f32 = 36.0;
pub const PR_EMPTY_ICON: f32 = 17.0;
/// 26px — the head's `refresh` and `external` buttons (r6), a 14px icon.
pub const PR_ICON_BUTTON: f32 = 26.0;
pub const ICON_PR_BUTTON_GLYPH: f32 = 14.0;
/// 15px — a readiness or check row's mark; a pending or skipped check's
/// hollow ring is 11 inside it.
pub const PR_MARK: f32 = 15.0;
pub const PR_MARK_RING: f32 = 11.0;
/// 40px — a readiness row (icon 15, padding 0 12, 10px gaps).
pub const PR_READY_ROW_H: f32 = 40.0;
/// The checks listed before `Show N more`.
pub const PR_CHECKS_SHOWN: usize = 8;
/// A comment card's avatar: an 18px circle on `paint::SEND`'s grey, its
/// initial at 10px.
pub const PR_AVATAR: f32 = 18.0;
pub const FS_PR_AVATAR: f32 = 10.0;
/// Inline code in a comment card, a step over the card's `RAISED`.
pub const PR_CODE_CHIP: u32 = 0x333333;
/// 18px — a comment body's line (12.5 type).
pub const PR_BODY_LH: f32 = 18.0;
/// The lines a folded comment body shows.
pub const PR_BODY_LINES: usize = 3;
/// 16px — where a list item's text starts in a comment or the description,
/// its marker hanging dim in the box before it, 6px short of the text.
pub const PR_LIST_HANG: f32 = 16.0;
/// The lines the folded description shows: its opening heading, if it has
/// one, and the first lines under it.
pub const PR_DESCRIPTION_LINES: usize = 5;
/// How far a flush row's hover ground (a check, `Show more`, `N resolved`)
/// reaches past the column either side: its content keeps the section
/// heads' inset, and the ground still clears it.
pub const PR_ROW_BLEED: f32 = SPACE_1;
/// 26px — a reply's indent under its comment.
pub const PR_REPLY_INDENT: f32 = 26.0;
/// The ink a resolved comment card keeps.
pub const PR_RESOLVED_OPACITY: f32 = 0.6;
/// A turn of the `refresh` icon while a read is out, in steps.
pub const PR_SPIN_MS: u64 = 900;
pub const PR_SPIN_STEPS: usize = 18;

/// A toast's width.
pub const TOAST_W: f32 = 400.0;
/// The gap the kit fans a stack out by under the pointer. The stack holds
/// one toast (`max_items`), so this is only ever the kit's own spacing.
pub const TOAST_GAP: f32 = SPACE_3;

// -------------------------------------------- pane body and the row column

/// 16px — the inline padding every Pane strip shares.
pub const PANE_PAD_X: f32 = SPACE_4;
/// The Pane body's padding: 16px top, so the first line never kisses the
/// head rule, and half a row (10px) at the bottom (the prototype's `.tx`
/// `padding-bottom: calc(var(--lh)/2)`): an idle followed tail ends 10px
/// above the Composer's rule. The working line is not an overlay: it sits
/// in the Composer's stack and pushes the transcript up while it shows.
pub const BODY_PAD_T: f32 = SPACE_4;
pub const BODY_PAD_B: f32 = LH_UI / 2.0;
/// A Group Pane's body ends 14px over its compact Composer (SPEC §5.9, the
/// mockup's `padding-bottom: 14px`), so a Decision or failure strip at the
/// tail sits where it is drawn.
pub const BODY_PAD_B_GROUP: f32 = 14.0;

/// The Pane body's bottom padding: Solo's `BODY_PAD_B`, a Group Pane's
/// `BODY_PAD_B_GROUP`.
pub fn body_pad_b(solo: bool) -> f32 {
    if solo {
        BODY_PAD_B
    } else {
        BODY_PAD_B_GROUP
    }
}
/// 12px — the glyph box every transcript and Composer row hangs its mark in
/// (`❯`, a tool dot, the answer mark, an elbow).
pub const GLYPH_BOX: f32 = 12.0;
/// 8px — from the glyph box to the row's text.
pub const GUTTER_GAP: f32 = 8.0;
/// 20px — **C1**, the text column of every transcript and Composer row:
/// `GLYPH_BOX + GUTTER_GAP`. An elbow result sits at C2 = C1 + `ELBOW_INDENT`.
/// (On the grid the gutter is two cells, `GLYPH_GUTTER`.)
pub const GUTTER_W: f32 = GLYPH_BOX + GUTTER_GAP;
/// 8px — between a row's glyph column and its text (tool rows, the working
/// line, the turn diff, controls beside a label).
pub const EVENT_GAP: f32 = 8.0;
/// 1px — the focused Pane's border (`FOCUS_RING` ink), lying exactly on the
/// Pane's own border box: focus changes colour and nothing else. Every
/// control's keyboard focus is the same 1px of the same ink, inset
/// (`components::control_focus`).
pub const FOCUS_RING_W: f32 = 1.0;

// ------------------------------------------------ shared controls and rows

/// 28px — an icon button's width (the prototype's `.ib`): the bottom
/// bar's doors, the bar's full height.
pub const ICON_BUTTON: f32 = 28.0;
/// 15px — an icon button's glyph, centred in its door.
pub const ICON_BUTTON_GLYPH: f32 = 15.0;
/// 12px — the chevron beside a picker or a disclosure.
pub const ICON_CHEVRON: f32 = 12.0;
/// 28px — a text button in pane and nav chrome, decisions and footers, with
/// 12px inline padding. Sheet controls are `FORM_CONTROL_H`.
pub const CONTROL_H: f32 = 28.0;
pub const CONTROL_PAD_X: f32 = SPACE_3;
/// **The float's shell** (SPEC §5.8 "Other floats": the palette's at a
/// smaller scale): radius `R_BLOCK`, rows `FLOAT_ROW_H` (`LH_UI` and 5px
/// above and below), each inset `FLOAT_PAD` from the float's edge with its
/// text `MENU_ROW_PAD_X` inside its fill, so a line without a fill (a
/// head, a note, a footer) sets its text `FLOAT_PAD_X` in.
pub const FLOAT_ROW_H: f32 = 30.0;
pub const FLOAT_PAD: f32 = SPACE_1;
pub const FLOAT_PAD_X: f32 = FLOAT_PAD + MENU_ROW_PAD_X;
/// The gap a float keeps from the control that opened it.
pub const FLOAT_OFFSET: f32 = 6.0;
/// The context menu's width, and any floating list's height cap before it
/// scrolls.
pub const MENU_W: f32 = 256.0;
pub const MENU_MAX_H: f32 = 420.0;
/// A menu row's inline padding and the gap between its label and trailing
/// parts; its fill's radius (`R_TAB`: concentric enough inside the float's
/// 8 at a 4px inset).
pub const MENU_ROW_PAD_X: f32 = SPACE_2;
pub const MENU_ROW_GAP: f32 = SPACE_3;
pub const R_MENU_ROW: f32 = R_TAB;
/// A section heading: 28px, its 12/500 word sat on the heading's foot
/// (4px under it) so it hugs the rows it heads, as the palette's does.
pub const FLOAT_SECTION_H: f32 = 28.0;
/// 8px — what splits one group of rows from the next inside any floating
/// surface. Space, never a rule.
pub const MENU_GROUP_GAP: f32 = SPACE_2;
/// An aligned name column (slash commands): clamped between these.
pub const MENU_NAME_MIN_W: f32 = 96.0;
pub const MENU_NAME_MAX_W: f32 = 220.0;
/// The folder and branch marks on the Project and checkout lines (12px), and
/// the 5px gap to their labels.
pub const ROW_ICON: f32 = 12.0;

// ------------------------------------------------------- status and motion

/// The room a typed status dot (`components::status_dot`) takes in a row:
/// its one cell.
pub const STATUS_DOT: f32 = CH;
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

/// The chrome's face, **bundled**: Geist (rule 1) — nav, heads, tabs,
/// buttons, menus, the palette, the panel, the bar. The kit's
/// `font_family` is set from it.
///
/// gpui has no variation-axis support, so `main.rs` registers static
/// instances (Regular, Italic, Medium, SemiBold, Bold) of both faces. Each
/// face's instances share their typographic family name (name ID 16), and
/// CoreText/DirectWrite resolve the instance from `.font_weight(..)`.
/// **Never reach a weight by family name**: `.font_family("Geist Medium")`
/// silently resolves to the fallback face.
pub const FONT_UI: &str = "Geist";

/// The transcript's face, and code, machine output and terminal numbers
/// anywhere: Geist Mono, the kit's `mono_font_family`.
pub const FONT_CODE: &str = "Geist Mono";

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
    // Two faces (rule 1): Geist for the chrome, Geist Mono for code.
    theme.font_family = FONT_UI.into();
    theme.font_size = px(FS_UI);
    theme.mono_font_family = FONT_CODE.into();
    theme.mono_font_size = px(FS_UI);
    // Radii by role (rule 2).
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
    theme.background = rgb(CARD).into();
    theme.foreground = rgb(TEXT).into();
    theme.border = paint::LINE2.into();
    // Kit menu and completion rows sit on a float: their hover and cursor
    // face is the selection overlay.
    theme.accent = paint::SELECTED.into();
    theme.accent_foreground = rgb(TEXT_STRONG).into();
    theme.secondary = rgb(RAISED).into();
    theme.secondary_hover = rgb(SELECTED).into();
    theme.secondary_active = rgb(SELECTION_HOVER).into();
    theme.secondary_foreground = rgb(TEXT).into();
    // The primary has no hue: the strongest ink, stepping down to `TEXT`
    // under the pointer and pressed.
    theme.primary = rgb(PRIMARY).into();
    theme.primary_hover = rgb(TEXT).into();
    theme.primary_active = rgb(TEXT).into();
    theme.primary_foreground = rgb(ON_PRIMARY).into();
    // `Button::primary` reads its own fields, not `primary`.
    theme.button_primary = rgb(PRIMARY).into();
    theme.button_primary_hover = rgb(TEXT).into();
    theme.button_primary_active = rgb(TEXT).into();
    theme.button_primary_foreground = rgb(ON_PRIMARY).into();
    theme.muted = rgb(RAISED).into();
    theme.muted_foreground = rgb(TEXT_DIM).into();
    theme.popover = paint::FLOAT.into();
    theme.popover_foreground = rgb(TEXT).into();
    theme.ring = rgb(CONTROL_FOCUS).into();
    theme.caret = rgb(CARET).into();
    // The reader's line numbers sit on the plane itself. The gutter is
    // painted opaque over the text, so it takes the plane's own colour.
    let mut highlight = (*theme.highlight_theme).clone();
    highlight.style.editor_gutter_background = Some(rgb(CARD).into());
    theme.highlight_theme = std::sync::Arc::new(highlight);
    theme.selection = rgba(TEXT_SELECTION_WASH).into();
    theme.link = rgb(LINK_INK).into();
    theme.link_hover = rgb(LINK_INK).into();
    theme.link_active = rgb(LINK_INK).into();
    // Native checkbox/radio indicators use `input` for their resting edge.
    theme.input = rgb(INPUT_EDGE).into();
    theme.switch = rgb(RAISED).into();
    theme.switch_thumb = rgb(TEXT_STRONG).into();
    theme.overlay = rgba(VEIL).into();
    theme.danger = rgb(BLOCKED).into();
    theme.warning = rgb(ATTENTION).into();
    theme.success = rgb(RUNNING).into();
    theme.info = rgb(TEXT_STRONG).into();
    theme.list_hover = paint::RAISED.into();
    theme.list_active = paint::SELECTED.into();
    theme.table_head = rgb(CARD).into();
    theme.table_head_foreground = rgb(TEXT_STRONG).into();
    theme.drag_border = rgba(DROP_EDGE).into();
    theme.drop_target = rgba(DROP_WASH).into();
    theme.sidebar = rgb(FRAME).into();
    theme.sidebar_foreground = rgb(TEXT).into();
    // The selected Settings page is the selection row.
    theme.sidebar_accent = rgb(SELECTED).into();
    theme.sidebar_accent_foreground = rgb(TEXT_STRONG).into();
    theme.sidebar_border = rgba(TRANSPARENT).into();
    // No track: only the thumb is ever ink, and it lightens rather than
    // darkens when the pointer takes hold of it.
    theme.scrollbar = rgba(TRANSPARENT).into();
    theme.scrollbar_thumb = rgb(SCROLLBAR).into();
    theme.scrollbar_thumb_hover = rgb(SCROLLBAR_HOVER).into();
    // Kit tabs, should one appear: no bar ground, the active tab `SELECTED`
    // in `TEXT_STRONG`, the rest muted.
    theme.tab_bar = rgba(TRANSPARENT).into();
    theme.tab_active = rgb(SELECTED).into();
    theme.tab_active_foreground = rgb(TEXT_STRONG).into();
    theme.tab_foreground = rgb(TEXT_DIM).into();
    // `Theme::change` resolved the kit's pre-computed tokens from its default
    // palette, and nothing recomputes them: widgets that read `tokens.*`
    // (Button::primary, menu rows, tooltips, checkboxes) would paint the
    // kit's neutrals. Rebuild them from the colours above.
    theme.tokens = gpui::component::ThemeTokens::from(&theme.colors);
    // A toast is the rail's voice only (C8): with the nav open its rows'
    // marks are the queue, and a toast shows only while the nav is
    // collapsed and its Thread is off the board (`NeedsYou::present`). It
    // stands BottomRight, above the Composer, one at a time; the Needs-you view
    // holds the rest.
    theme.notification.placement = gpui::Anchor::BottomRight;
    theme.notification.margins = gpui::base::Edges {
        top: px(WIN_CHROME_H + GRID_PAD),
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

// ======================================== WP-A · transcript rows and grammar
// Owner: WP-A (transcript.rs, pane/text.rs, the transcript rows in pane.rs, ferrite-core transcript strings.)
// Edit values and append tokens only inside this section.

// ---------------------------------------------------- transcript grammar
//
// The transcript is a terminal: Claude Code's own layout on one monospace
// grid, in Ferrite's ink, inside the Pane's card (SPEC §5.4).
//
// - **One grid.** Every transcript row is set at the operator's reading
//   size on its 1.6× line (`answer_text_size`, `answer_line_height`) in
//   Solo, `TX_GROUP_STEP` smaller in a Group Pane (`tx_size`), set once on
//   the transcript list and inherited by every row: prose, tool calls,
//   results, the stamp. Diffs and fences are set `TX_BLOCK_STEP` smaller
//   again. Horizontal measures are cells at that size (`tx_cell`);
//   vertical ones are its line and half line.
// - **The 2-cell gutter.** Every row is `[gutter | content]`: the mark in
//   the gutter's first cell (`tx_gutter`, two cells wide), the content
//   column after it, and wrapped lines hang under the content column, never
//   under the mark. Rows sit `tx_inset` (28px Solo, 18px Group) from the
//   card's edges; there is no centred column.
// - **Marks.** `❯` (`TEXT_STRONG`, semibold) heads the operator's prompt; `●`
//   heads agent prose (`TEXT_STRONG`, once per speaker change:
//   `transcript::AnswerMarks`) and every tool call (`TEXT_DIM` done,
//   `RUNNING` live, `BLOCKED` failed; never pulsing); `✻` (in the
//   provider's colour) the turn's end; `◆` (`ATTENTION`) a Decision — all
//   typed (rule 10). `∴` (drawn) heads reasoning.
// - **The banner.** Every transcript opens on one `TEXT_DIM` line:
//   `~/ferrite on dev · started 7:18 pm` (ADR 0014). The nav row names
//   the Thread and the status line its model.
// - **The prompt band.** The prompt echo is a `paint::FIELD` box
//   (`R_CONTROL`), `PROMPT_BAND_PAD_X` either side and pulled out by as
//   much so its text keeps the column, `prompt_band_pad_y` above and below:
//   `❯` (`W_STRONG`) in the gutter, the text in `TEXT_STRONG` at body
//   weight, its attachments inline after the words (`TEXT_STRONG` names on
//   `paint::RAISED` chips at `R_INLINE`), and its send time (`7:31 pm`,
//   `TEXT_DIM`) in its own right-aligned column on the first line. The
//   band of the turn being read stays pinned at the top of the body while
//   its output scrolls under it — exactly the band, on the card, pushed up
//   by the next turn's band. Nothing on a band answers the pointer.
// - **Tool rows.** `● Name(args)`, one row per call (never folded into a
//   summary): the name `TEXT_STRONG` at `W_STRONG` (`Update`, `Bash`, the
//   way Claude Code names them), the parens and arguments `TEXT_DIM` (a
//   path argument `TEXT_DIM`, a hover target), one line that truncates; the
//   trail hard right (`+N −M` in `RUNNING`/`BLOCKED`, then the duration
//   `TEXT_DIM`, tabular). What it produced hangs under a typed `└ ` elbow
//   (`TEXT_FAINT`) on the content column, in `TEXT_DIM`; further output
//   lines align after the elbow. Long output folds to
//   `OUTPUT_PREVIEW_LINES` and a `+ N lines` line that toggles it. A tool
//   row changes nothing under the pointer but a path's underline.
// - **Rhythm in rows.** Blocks are one blank line apart (a turn's prompt
//   band, prose, a run of tool calls, the stamp); the calls of one run and
//   the rows that hang on an elbow under the row they answer sit flush,
//   except that a call after a shown diff sits a line below it. The space
//   above a row is chosen at reconcile from the row before it
//   (`rows::gap_before`) and is part of the row's identity.
// - **The turn's end.** `✻ Worked for 41s · 7:32 pm · ↑ 3.2k ↓ 1.1k`,
//   `TEXT_DIM`, every `·` too. A failed or interrupted turn hangs under
//   the last row: `└ failed · 0.1s · <message>`, the lead word in its state
//   ink.
// - **Diffs** sit in the block box (`BLOCK_*`: `paint::BAR`, `R_CONTROL`,
//   an inset `HAIRLINE`), `BLOCK_MARGIN_T` under their call: a `TEXT_DIM`
//   hunk header naming its section (none when it has none), line numbers
//   right-aligned in `DIFF_NUMBER_CELLS` (`TEXT_FAINT`, or
//   `DIFF_*_NUMBER` on a changed row), a `DIFF_SIGN_CELLS` sign carrying
//   the hue, red and green row washes (`DIFF_*_WASH`) with the changed
//   tokens washed deeper (`DIFF_*_WORD`), code in full syntax colour and
//   one line per row, cut with `…`. Side by side once the transcript
//   is `SPLIT_DIFF_MIN_W` wide, unified when narrower; a split row's empty
//   side is `paint::NODIFF`. A later edit to a file already diffed in the
//   turn shows `+ show diff` instead.
// - **No transcript row has a hover ground** but the keyboard's disclosure
//   target (`paint::RAISED`).

/// A prose-relative vertical step at answer size `size`: em-proportional to
/// the Standard prose size, whole pixels. `GAP_TURN`, `GAP_BLOCK` and the
/// Markdown gaps go through it; UI-row steps do not.
#[cfg(test)] // the previous grammar, now read only by tests
pub fn reading_step(step: f32, size: f32) -> f32 {
    (step * size / FS_PROSE).round()
}

/// A disclosed call echoes its input under `⎿` only when the call line could
/// not show it whole: a command (always, exactly), a titled call, a
/// multi-line input, or one longer than this many characters.
pub const INPUT_ECHO_CHARS: usize = 48;
pub use ferrite_core::progress::DURATION_MIN_MS;
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
/// A diff row's wash: green and red at 9% (SPEC §3.4, the mockups'
/// add and delete rows); the changed words wash deeper (`DIFF_*_WORD`, in the head).
/// (`BLOCKED_WASH` is for one-line uses only.)
pub const DIFF_ADDED_WASH: u32 = 0x93cf8c17;
pub const DIFF_REMOVED_WASH: u32 = 0xef8a8017;
/// How many rows one hunk card draws before it stops and says how many it
/// did not. An edit's patch is a handful of lines; a written file's is
/// however long the file is, and a card that redrew a 900-line file would
/// be the transcript rather than a note in it.
pub const HUNK_MAX_ROWS: usize = 24;

/// 20px — an invisible hit area, not a drawn thing: a disclosure's gutter
/// target (the whole row toggles too) and a prompt action's button.
pub const TOOL_DISCLOSURE_HIT: f32 = GUTTER_W;
/// One cell of the transcript grid at reading size `size`: Geist Mono's
/// advance (`CODE_ADVANCE`), 7.8px at Standard.
pub fn tx_cell(size: f32) -> f32 {
    size * CODE_ADVANCE
}

/// The transcript's 2-cell glyph gutter at reading size `size`: the row's
/// mark in its first cell, the content column after its second.
pub fn tx_gutter(size: f32) -> f32 {
    2.0 * tx_cell(size)
}

/// A drawn transcript mark (`∴`) at reading size `size`: sized like a glyph
/// of the face, 12px at Standard (`GLYPH_BOX`).
pub fn tx_mark(size: f32) -> f32 {
    (size * GLYPH_BOX / FS_PROSE).round()
}

/// Half a transcript line, whole pixels: the prompt band's padding above
/// and below its text.
pub fn tx_half(line: f32) -> f32 {
    (line / 2.0).floor()
}

/// The transcript column's inset from the card's edges, left and right
/// (SPEC §5.4): 28px in Solo, 18px in a Group Pane. Off the space scale:
/// the mockups' `padding: 0 28px` / `0 18px`, which put the prompt band
/// (pulled out `PROMPT_BAND_PAD_X`) 16px and 6px in from the card. The
/// right inset clears the minimap rail (`MINIMAP_W`).
pub const TX_INSET_SOLO: f32 = 28.0;
pub const TX_INSET_GROUP: f32 = 18.0;
const _: () = assert!(TX_INSET_GROUP - PROMPT_BAND_PAD_X > 0.0);

/// The transcript column's inset for a Solo (`solo`) or a Group Pane.
pub fn tx_inset(solo: bool) -> f32 {
    if solo {
        TX_INSET_SOLO
    } else {
        TX_INSET_GROUP
    }
}

/// **The prompt band** (SPEC §5.4): a `paint::FIELD` box, `R_CONTROL`
/// corners, `PROMPT_BAND_PAD_X` either side and pulled out by as much, so
/// its `❯` and words keep the transcript's column; `PROMPT_BAND_PAD_Y`
/// above and below in Solo, `PROMPT_BAND_PAD_Y_GROUP` in a Group Pane (the
/// mockups' `padding: 8px 12px` / `7px 12px`: one off-scale pixel less on
/// the Group's 20px line).
pub const PROMPT_BAND_PAD_X: f32 = SPACE_3;
pub const PROMPT_BAND_PAD_Y: f32 = SPACE_2;
pub const PROMPT_BAND_PAD_Y_GROUP: f32 = 7.0;

/// The prompt band's padding above and below its text, Solo or Group.
pub fn prompt_band_pad_y(solo: bool) -> f32 {
    if solo {
        PROMPT_BAND_PAD_Y
    } else {
        PROMPT_BAND_PAD_Y_GROUP
    }
}

/// **The block box** — a diff's or a code fence's (SPEC §5.4): `paint::BAR`
/// ground, `R_CONTROL` corners, an inset 1px `HAIRLINE`, `BLOCK_PAD_Y`
/// above its first line and below its last, `BLOCK_MARGIN_T` under the row
/// it hangs from.
pub const BLOCK_PAD_Y: f32 = SPACE_1_5;
pub const BLOCK_MARGIN_T: f32 = SPACE_1_5;
/// A diff line's number ink on an added or a removed row (`TEXT_FAINT` on
/// a context row): the sign's hue, dimmed to structure.
pub const DIFF_ADDED_NUMBER: u32 = 0x5f8a66;
pub const DIFF_REMOVED_NUMBER: u32 = 0x8a6460;
/// The gap between a diff line's number and its sign: the mockups'
/// `padding-right: 1.5ch`, inside the number's `DIFF_NUMBER_CELLS`.
pub const DIFF_NUMBER_PAD_CELLS: f32 = 1.5;
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
/// An inline image's frame in cells, its 1px `paint::LINE2` border included
/// (the prototype's `.img{width:48ch}`, 374.4px at Standard); the picture
/// fills it.
pub const IMAGE_CELLS: f32 = 48.0;

/// **The minimap** (the transcript's scrollbar): a `MINIMAP_W` rail at the
/// body's right edge, shown while the pointer is anywhere on the Pane
/// (Composer included) and fading over `MOTION_HOVER_FADE_MS`. A
/// `MINIMAP_TICK_H` tick (`MINIMAP_TICK_R` corners) per prompt (`TEXT_STRONG`),
/// failed call (`BLOCKED`), Decision (`ATTENTION`) and passing check
/// (`RUNNING`), inset `MINIMAP_TICK_INSET` — 6px wide; the viewport as a
/// white band at 6% (`MINIMAP_BAND`), 11% under the pointer
/// (`MINIMAP_BAND_HOVER`), `MINIMAP_BAND_R` corners, inset
/// `MINIMAP_BAND_INSET`, never shorter than `MINIMAP_BAND_MIN_H`; the band
/// hides while the whole transcript fits, the ticks stay. A click on a tick
/// scrolls smoothly to its row; anywhere else centres the view on that
/// point.
pub const MINIMAP_W: f32 = 12.0;
pub const MINIMAP_TICK_H: f32 = 2.0;
pub const MINIMAP_TICK_INSET: f32 = 3.0;
pub const MINIMAP_TICK_R: f32 = 1.0;
pub const MINIMAP_BAND_INSET: f32 = 1.0;
pub const MINIMAP_BAND_MIN_H: f32 = 12.0;
pub const MINIMAP_BAND_R: f32 = 2.0;
pub const MINIMAP_BAND: u32 = 0xffffff0f;
pub const MINIMAP_BAND_HOVER: u32 = 0xffffff1c;
/// A minimap jump's scroll: eased over this long (reduced motion jumps).
pub const MINIMAP_SCROLL_MS: u64 = 300;

/// **The banner** (a transcript's first row): one muted line; Solo pads
/// `BANNER_PAD_T` above it (board Panes none, `.tx{padding-top:0}`), and the
/// first band sits a line below it.
pub const BANNER_PAD_T: f32 = LH_UI;
/// The prompt band's send time: its own right-aligned column, this many
/// cells of padding on its left (the prototype's `.when{padding-left:2ch}`).
pub const PROMPT_TIME_PAD_CELLS: f32 = 2.0;
/// What a prompt band's chip line adds below it: the prototype's chip is an
/// `inline-flex` box seated on its icon's synthesized baseline, which lifts
/// it above the line's strut and grows that line box by two pixels. The
/// chips close the prompt, so the band takes the extra under its last line.
pub const PROMPT_CHIP_LINE_EXTRA: f32 = 2.0;
/// A running test's bar (the prototype's `.bar`): `TEST_BAR_CELLS` wide,
/// `TEST_BAR_H` tall on a `paint::LINE2` track, `RUNNING` fill at sub-pixel
/// precision easing over `TEST_BAR_EASE_MS`, a cell's margin each side,
/// `TEST_BAR_R` corners.
pub const TEST_BAR_CELLS: f32 = 20.0;
pub const TEST_BAR_H: f32 = 6.0;
pub const TEST_BAR_R: f32 = 1.0;
pub const TEST_BAR_EASE_MS: u64 = 300;
/// A fold's height eases open or shut over this long (the prototype's
/// `.fold{transition:grid-template-rows .22s}`); reduced motion snaps.
pub const FOLD_EASE_MS: u64 = 220;
/// **The hover card** (the prototype's `#hover`): `HOVER_CARD_CELLS` chrome
/// cells wide (780px), `HOVER_CARD_GAP` below the hovered path (above when
/// it would overflow), at most `HOVER_CARD_EDGE` from the cockpit's right
/// edge; six numbered lines (`HOVER_CARD_LINES`), the number column
/// `HOVER_CARD_NUMBER_CELLS` wide with `HOVER_CARD_NUMBER_PAD_CELLS` after.
pub const HOVER_CARD_CELLS: f32 = 100.0;
/// (The prototype measures its 6px from the path's inline box — the face's
/// content area, which ends 2.5px above the 20px row's foot; the card hangs
/// from the row, so 3.5 here.)
pub const HOVER_CARD_GAP: f32 = 3.5;
pub const HOVER_CARD_EDGE: f32 = 16.0;
pub const HOVER_CARD_LINES: u32 = 6;
pub const HOVER_CARD_NUMBER_CELLS: f32 = 6.0;
pub const HOVER_CARD_NUMBER_PAD_CELLS: f32 = 2.0;
/// A path's hover underline (the prototype's `.path`): `PATH_UNDERLINE_W`
/// thick, `PATH_UNDERLINE_OFFSET` below the baseline, in the path's own
/// ink, fading in over `PATH_UNDERLINE_FADE_MS`; transparent at rest.
pub const PATH_UNDERLINE_W: f32 = 1.0;
pub const PATH_UNDERLINE_OFFSET: f32 = 3.0;
// (end WP-A) — append above this line only

// ======================================== WP-B · markdown, prose, scrollbars
// Owner: WP-B (rich.rs, scrollbar.rs, attachments::inline_file, the Markdown vendor knobs.)
// Edit values and append tokens only inside this section.

/// **Markdown** (SPEC §5.4). Agent prose is the one face in `TEXT_PROSE`
/// at the transcript's size, left-aligned on the content column and held to
/// `MEASURE_CH` cells; code, tables and rules keep the whole column. Blocks
/// sit one blank line apart; headings are the body size in `W_STRONG`
/// `TEXT_STRONG`, `**strong**` the same. Inline code is `INLINE_CODE_INK`
/// on its `RAISED` chip; links and file paths are `LINK_INK` with no
/// underline at rest, every link underlining under the pointer
/// (`PATH_UNDERLINE_*`). A list's `•` is `TEXT_DIM`, hanging in the first
/// of two cells. A fence is the diff's block box (`BLOCK_*`), the code
/// `CODE_PAD_CELLS` in, `TX_BLOCK_STEP` under the prose, in full syntax
/// colour, at most `MEASURE_CH` cells from the content column; its language
/// tag dim on its first line at the right, `Copy` before the tag blending
/// in under the pointer. A table takes its content's width: a `W_STRONG`
/// `TEXT_STRONG` head over a `paint::LINE2` rule, row rules in `paint::LINE`
/// (each row exactly one line), no vertical rules and no box, the first
/// column flush, figures tabular; a column of numbers aligns right (head
/// included) unless the Markdown says otherwise. An image is framed like a
/// prompt's (`IMAGE_CELLS`) with its caption under it. A quote is a 2px
/// `LINE2` rule and `TEXT_DIM`; a thematic break one `LINE`.
///
/// 12px — between Markdown blocks (`SPACE_3`), the transcript's block
/// step. This and the heading spaces are Standard values; other reading
/// sizes scale them with `reading_step`.
#[cfg(test)] // the previous grammar, now read only by tests
pub const PROSE_GAP: f32 = SPACE_3;
/// Inline code's ink: `TEXT_STRONG` on its chip (cyan is retired, ADR
/// 0033). The Markdown path paints both; the plain-text fallback carries
/// the ink alone.
pub const INLINE_CODE_INK: u32 = TEXT_STRONG;
/// The inline-code chip (`rich::inline_code_wash`): a `paint::RAISED`
/// ground at `R_INLINE`, with `INLINE_CODE_PAD_X` of room laid out either
/// side of the code (the text after a chip moves along, as the mockups'
/// `padding: 1px 5px` moves it; the copied text is unchanged) and
/// `INLINE_CODE_INSET_Y` inside the line box. Off-scale, as the mockups
/// draw it.
pub const INLINE_CODE_PAD_X: f32 = 5.0;
pub const INLINE_CODE_INSET_Y: f32 = 1.0;

/// A quote's rule. Its text inset is the hang less the rule
/// (`PROSE_HANG − QUOTE_RULE_W`, 26 at Standard), so quoted text starts
/// where list text does.
pub const QUOTE_RULE_W: f32 = 2.0;
/// 4px — a horizontal rule's own margin inside its block, so it sits 16px
/// from its neighbours.
pub const RULE_MARGIN_Y: f32 = SPACE_1;
/// A fence's code sits this many cells (at the fence's size) in from the
/// block's sides. Its actions overlay — the language id, html `Preview`,
/// `Copy`/`Copied` — keeps the same inset at the right and stands on the
/// code's first line (`BLOCK_PAD_Y` down, one code line high), the id set
/// as the code is, on its baseline. The overlay is always laid out (so it
/// never moves the block); the actions show only under the pointer (the
/// 150ms hover blend), while their keys have focus, or while the caret or
/// a selection is inside the block — those two instantly.
pub const CODE_PAD_CELLS: f32 = 2.0;
/// Code actions keep a stable target when Copy becomes Copied.
pub const CODE_ACTION_MIN_W: f32 = 56.;
/// A reading dialog (the html preview, What's New): the reading column's
/// width, and a height cap before its body scrolls.
pub const READING_DIALOG_MAX_H: f32 = 520.0;
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
// Owner: WP-C (the pane shell, head, wall tiles, board, seams, titlebar.)
// Edit values and append tokens only inside this section.
//
// **Cards on a frame** (theme rule 2, SPEC §4). The window is two rows: the
// nav and the board column over the head row (`WIN_CHROME_H`), and the
// full-width bottom bar (`STATUS_BAR_H`). The board column starts at the
// nav's right edge (no seam: the card's edge separates) and leaves
// `BOARD_PAD` of frame right of and under its cards; its left edge rides
// the nav's width every frame while cmd-B folds it (`motion::RESIZE`). The
// column paints the frame itself (`components::frame_around`): `paint::
// FRAME` over the head row and the gutters, with a rounded hole per card,
// so the frame and the cards are separate regions (rule 4) and whatever a
// card's children paint past its rounded corners is covered. Cards on a
// board are `BOARD_SEAM` apart; a seam is the gutter's centre line and its
// 9px grab band (`SEAM_GRAB` in the cockpit) that resizes its two sides,
// clamped to 22–78% of the board. A default grid's aligned seams are one
// line that drags as one, and under the pointer and while held the whole
// line draws `SEAM_LINE_W` of `SEAM_HOVER` in the gutter, fading in over
// `MOTION_SEAM_FADE_MS`.
//
// **The Pane card.** `paint::CARD`, radius `R_PANE`, its content starting at
// its own edges; its 1px ring is an overlay just outside them (the mockups'
// `box-shadow: 0 0 0 1px`), so a state change reflows nothing. By
// precedence (`pane::PaneEdge`): a waiting Pane `WAIT_RING`, focused or
// not > focused `FOCUS_RING` (only while more than one Pane is on the
// board) > at rest `CARD_RING`. Every Pane root is the `pane::PANE_GROUP`
// hover group.
//
// **Solo has no head**: the nav's selected row names the Thread and the
// body starts at the plane's top edge.
//
// **The Group head** (`pane::group_head`, SPEC §5.9) is one 38px row
// (`PANE_HEAD_H`) closed by a `HAIRLINE`, on every board Pane: the row's
// mark (`nav::head_mark`), the title at `W_LABEL` — `TEXT_STRONG` on the
// focused Pane only — its state word in 12 (`needs you` `ATTENTION`,
// `failed` `BLOCKED`, the elapsed time and an unseen `done` `TEXT_DIM`),
// then the provider's 13px logo and the fill button. No band on the
// focused head: the ring says focus. A draft carries its × instead.
//
// **Two altitudes** (R12): a Pane reads its transcript from 300px wide and
// 360px tall (`docview::Level::for_cell`); smaller, it is a wall tile. One
// Level per board (rule 2.3.5), with `LEVEL_HYSTERESIS`. A tile is the
// head, its state word at the strong weight in its colour, half a row, its
// last `WALL_LINES` lines — one `TEXT_DIM` run each, glyph included —
// and, while it waits, quick answers boxed in `paint::LINE2`; no meter, no
// sparkline.
//
// **The window chrome** (ADR 0014, SPEC §4). The head row is the nav's top
// chrome, the traffic lights' row and the window's drag strip, and the slot
// the view head and the panel's tabs fill; the cards start below it. The window's one
// bar is the bottom bar (`STATUS_BAR_H`), on the chrome under a
// `paint::LINE` rule: the sidebar toggle and the bell, the session in
// `TEXT_STRONG` at the strong weight, `1 solo` and a tab per Group (the current
// view's `TEXT_STRONG` on `paint::RAISED`), the focused Pane's status line
// (mode, model, `ctx`, changed files), each provider's usage behind its
// mark, `?` and the clock. No state counts.
//
// **Text rasterisation** (F-17). The prototype's glyphs are greyscale with
// no stem darkening (`-webkit-font-smoothing: antialiased`); gpui-pre-macos
// thickens light ink on dark ground whenever `AppleFontSmoothing` is not an
// explicit 0 for the app, read once per process. `platform_text::
// disable_font_smoothing` sets it to 0 in the app's own domain before the
// first glyph (from `main`, and from a scene before its first render), so
// `done`, titles and the state words carry the prototype's weight.

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
/// The smallest window the chrome still lays out in: the nav plus one wall
/// tile and the Windows caption group.
pub const WINDOW_MIN_W: f32 = 640.0;
pub const WINDOW_MIN_H: f32 = 420.0;

/// How far every cell must clear a Level's size threshold, on both axes,
/// before the board steps *up* to it (`CockpitView::board_level`); it steps
/// down at the plain threshold. 24px keeps a resize that hovers at an edge
/// from flickering the whole board between tiers.
pub const LEVEL_HYSTERESIS: f32 = 24.0;

/// 8px — the gutter between two cards on a board, the tree's gap
/// (`layout::Tree::rects`): the core's `GRID_GAP`, so the default grid's
/// equal cells stay equal. Its centre line is the seam.
pub const BOARD_SEAM: f32 = GRID_GAP;
/// 2px — the line a seam draws in its gutter under the pointer and while
/// held (`SEAM_HOVER`); at rest the gutter alone separates the cards.
pub const SEAM_LINE_W: f32 = 2.0;

/// 38px — the Group head (SPEC §5.9; a reader's head is its twin): the
/// card's tab strip height, its `HAIRLINE` inside it.
pub const PANE_HEAD_H: f32 = 38.0;
/// A dragged slot's ghost head's inline inset: one cell.
pub const HEAD_PAD_X: f32 = CH;
/// The Group head's insets and gap (and its twin's, the reader's head): 14
/// before the mark, 6 after the fill button or the reader's × (its 28px
/// hit box carries the rest of the visual inset), 8 between items.
pub const PANE_HEAD_PAD_L: f32 = 14.0;
pub const PANE_HEAD_PAD_R: f32 = SPACE_1_5;
pub const PANE_HEAD_GAP: f32 = SPACE_2;
/// 13px — the provider's logo and a mark's icon in the Group head.
pub const PANE_HEAD_ICON: f32 = 13.0;
/// The width a dot mark lays out at in the Group head: the dot and a pixel
/// each side, as the mockup's box (a halo overflows it, as a CSS
/// `box-shadow` does).
pub const PANE_HEAD_DOT_W: f32 = NAV_DOT + 2.0;
/// 14px — the fill button's `maximize` glyph.
pub const PANE_HEAD_FILL_GLYPH: f32 = 14.0;

/// The compact Composer (SPEC §5.9 item 5, `pane::compact_composer`): a
/// 38px box `COMPACT_COMPOSER_INSET` in from the card's sides and foot,
/// padded 12 before the input and 7 after the send circle (24, its
/// `arrow-up` 12), `COMPACT_WORKING_GAP` under the working line.
pub const COMPACT_COMPOSER_H: f32 = 38.0;
pub const COMPACT_COMPOSER_INSET: f32 = 10.0;
pub const COMPACT_COMPOSER_PAD_L: f32 = SPACE_3;
/// Off the space scale: centres the 24px circle 7px from the box's edge,
/// as the mockup draws it.
pub const COMPACT_COMPOSER_PAD_R: f32 = 7.0;
pub const COMPACT_WORKING_GAP: f32 = 14.0;
pub const SEND_CIRCLE_SM: f32 = SPACE_6;
pub const SEND_GLYPH_SM: f32 = SPACE_3;

/// The failure strip (SPEC §5.9 item 4): 36 high at `R_BLOCK` on
/// `FAILURE_WASH`, padded 12 / 6, its `x-circle` 14, its `Retry` 26 high
/// at `R_TAB`, padded 10.
pub const FAILURE_STRIP_H: f32 = 36.0;
pub const FAILURE_STRIP_PAD_L: f32 = SPACE_3;
pub const FAILURE_STRIP_PAD_R: f32 = SPACE_1_5;
pub const FAILURE_STRIP_ICON: f32 = 14.0;
pub const RETRY_H: f32 = 26.0;
pub const RETRY_PAD_X: f32 = 10.0;
/// The floor a head title keeps however narrow the head (a shorter title
/// keeps its whole text). There is no cap — a long title takes the width
/// the head has.
pub const HEAD_TITLE_MIN_W: f32 = 96.0;
/// The tasks meter: 6 × 3 segments, 1px radius, 2px apart (an 8px pitch).
/// Past `METER_SEG_CAP` steps it is one `METER_TRACK_W` track.
pub const METER_SEG_W: f32 = 6.0;
pub const METER_SEG_H: f32 = 3.0;
pub const METER_SEG_GAP: f32 = SPACE_0_5;
pub const METER_SEG_R: f32 = 0.0;
pub const METER_SEG_CAP: usize = 12;
pub const METER_TRACK_W: f32 = 48.0;
/// Between the meter and its `3/4` count.
pub const METER_GAP: f32 = CH;
/// The checks card (`open_checks`, a palette command): wide enough for a
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
pub const CHECKS_HEAD_H: f32 = FLOAT_ROW_H;
pub const CHECKS_ROW_H: f32 = FLOAT_ROW_H;
/// The space that sets a workflow's group off from the one before it. The
/// heading itself is the menu section title (`FLOAT_SECTION_H`).
pub const CHECKS_GROUP_GAP: f32 = MENU_GROUP_GAP;

/// The wall tile's body: half a row above and below, two cells in.
pub const WALL_PAD_Y: f32 = HALF_ROW;
pub const WALL_PAD_X: f32 = 2.0 * CH;
/// How many rows of a tile's last transcript lines it shows, dim, under its
/// state word (a wrapped prose line spends two).
pub const WALL_LINES: usize = 3;
/// A quick answer at the wall: `1 allow` as an edgeless secondary button,
/// 22px tall (a line and a pixel above and below), a cell of padding each
/// side, a cell between buttons, half a row above the row.
pub const QUICK_ANSWER_H: f32 = LH_UI + 2.0;
pub const QUICK_ANSWER_PAD_X: f32 = CH;
pub const QUICK_ANSWER_GAP: f32 = CH;

/// The reader beside a Pane (`cockpit/beside.rs`): line numbers in a
/// 6-cell column, right-aligned, two cells before the code.
pub const READER_NUMBER_W: f32 = 6.0 * CH;
pub const READER_NUMBER_GAP: f32 = 2.0 * CH;
/// The share of a Pane's slot a reader opening beside it takes, and how
/// far a drag can take the seam between them (the board's 22–78%).
pub const READER_SHARE: f32 = 0.5;

/// A dragged Pane's drop wash: `DROP_WASH` ground, `DROP_EDGE` edge; its
/// label is a float tag in the float's shell, one tooltip high.
pub const DROP_LABEL_PAD_X: f32 = TOOLTIP_PAD_X;
pub const DROP_LABEL_PAD_Y: f32 = TOOLTIP_PAD_Y;
/// The Pane a live drag picked up, dimmed in its slot until the release.
pub const DRAG_SOURCE_OPACITY: f32 = 0.5;
/// The miniature that rides the pointer while a slot is dragged: see-through
/// enough that the board under it still reads.
pub const DRAG_GHOST_OPACITY: f32 = 0.86;

/// The empty board, on the plane: two rows down and four cells in, a
/// banner (the mark three rows tall, `EMPTY_BANNER_GAP` before its lines),
/// then the commands and the recent Threads as rows on the grid.
pub const EMPTY_PAD_Y: f32 = 2.0 * ROW;
pub const EMPTY_PAD_X: f32 = 4.0 * CH;
pub const EMPTY_BANNER_GAP: f32 = 3.0 * CH;
pub const EMPTY_MARK_H: f32 = 3.0 * ROW;
/// A command row (fit-content): the `❯` column, the verb's 30 cells, the
/// key's 10 right-aligned, a cell of padding after it. A row's band (the
/// selection, a recent row's hover) hangs `EMPTY_ROW_INSET` outside the
/// text's edge, so its rounded fill clears the `❯`.
pub const EMPTY_ROW_INSET: f32 = SPACE_2;
pub const EMPTY_VERB_W: f32 = 30.0 * CH;
pub const EMPTY_KEY_W: f32 = 10.0 * CH;
/// A recent row (fit-content, 68 cells under the hover band): two cells of
/// air, the dot's two, the title's 44, the project's 14, the age's 6.
pub const EMPTY_RECENT_TITLE_W: f32 = 44.0 * CH;
pub const EMPTY_RECENT_PROJECT_W: f32 = 14.0 * CH;
pub const EMPTY_RECENT_AGE_W: f32 = 6.0 * CH;
/// How many recent Threads the empty board lists (R13).
pub const EMPTY_RECENT_MAX: usize = 4;

/// The provider mark on the empty board's provider rows.
pub const BAR_MARK: f32 = 11.0;
/// 14px — the bottom bar's inset at each end (SPEC §5.7; off the space
/// scale, as the mockup draws it).
pub const BAR_PAD_X: f32 = 14.0;
/// 18px — between the bottom bar's items (SPEC §5.7; off the space scale,
/// the mockup's).
pub const BAR_GAP: f32 = 18.0;
/// 22px — a view tab in the bottom bar, `SPACE_2` in from each side.
pub const BAR_TAB_H: f32 = 22.0;
pub const BAR_TAB_PAD_X: f32 = SPACE_2;
/// 5px — a view tab's roll-up dot (off the scale: a mark, the mockup's).
pub const BAR_TAB_DOT: f32 = 5.0;
/// 6px — the need-you count's dot.
pub const BAR_NEED_DOT: f32 = SPACE_1_5;

/// An empty draft's mark, flat, in a quiet ink a step over the plane.
pub const EMBOSS_ON_PANE: u32 = LINE;
/// 44px — an empty draft's mark over its Composer (Solo) or at its body's
/// centre (a board): the Pane is ready, and the Composer's placeholder
/// still says what to do, once.
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

/// **The Composer is a box in the card** (SPEC §5.5). In Solo it stands
/// `COMPOSER_INSET` from the card's sides and foot: `paint::FIELD` at
/// `R_PANE`, a 1px `FIELD_RING` laid outside it (it takes no layout).
///
/// - **The working line** (`pane::working_line`) is the stack's top row,
///   `COMPOSER_WORKING_GAP_SOLO` above what follows it.
/// - **The shelf** stands `SHELF_GAP` above the box its files are for:
///   pending files at left, background tasks at right, chips
///   `SHELF_CHIP_H` high at `R_CHIP` on `paint::RAISED`, `FS_META`.
/// - **The queued row** (`QUEUED_ROW_H`, when anything is queued): `Queued`
///   in `FS_META`/`W_LABEL` `TEXT_DIM`, the top of the pile in
///   `TEXT_MUTED` on one cut line, `+N` in `TEXT_DIM`, and a
///   `QUEUED_CLOSE` icon button that drops it. ↑ and ⌫ still act on it;
///   they are not printed. A `HAIRLINE` follows.
/// - **The input** is Geist `FS_COMPOSER` on `COMPOSER_ROW_H`, padded
///   `COMPOSER_INPUT_PAD_T` / `COMPOSER_INPUT_PAD_X` / `COMPOSER_INPUT_PAD_B`,
///   growing to `composer::MAX_ROWS` rows. No `❯`: the box says "type
///   here". The placeholder (`TEXT_DIM`) overlays its first row.
/// - **The caret is a bar** `CARET_W` wide and a row high at the cursor's
///   shaped x, in `CARET`, blinking softly (`MOTION_CARET_BLINK_MS`,
///   `components::caret_blink`) in Solo and the focused board Pane whoever
///   holds the keyboard; an unfocused board Pane draws none (SPEC §5.9).
///   Typing restarts the blink solid.
/// - **The toolbar** (`COMPOSER_TOOLBAR_H`, `COMPOSER_TOOLBAR_PAD_X`,
///   `COMPOSER_TOOLBAR_GAP`): the `+` attach button (`COMPOSER_ATTACH`), a
///   draft's Project and Workspace chips, the permission mode chip, a
///   spacer, the model chip, the effort chip, and the send circle
///   (`COMPOSER_SEND`, `paint::SEND`, `COMPOSER_SEND_GAP` after the chips).
///   A chip is `TOOL_CHIP_H` high at `R_CHIP`, `TOOL_CHIP_PAD_X` each side,
///   `FS_LABEL`, its parts `TOOL_CHIP_GAP` apart, `paint::RAISED` under the
///   pointer; its menu rests above it.
///
/// A Group Pane's Composer is the compact box (`COMPACT_COMPOSER_*`, SPEC
/// §5.9).
///
/// Any pad or line change here must update `pane::composer_fixed_height` in
/// the same commit.
/// The old board band's padding, kept for the kit's toast margin
/// (`TOAST_ABOVE_COMPOSER`).
pub const COMPOSER_PAD_Y: f32 = 6.0;
pub const COMPOSER_ROW_H: f32 = LH_UI;
pub const COMPOSER_STATUS_PAD_B: f32 = 6.0;
/// How far a table cell's text sits under the cell's top (`rich::style`):
/// the prototype's 19px line centred in its 20px cell, rounded down to the
/// whole pixel.
pub const TABLE_TEXT_DROP: f32 = 1.0;
/// The context meter's state steps (the panel's foot, `pane::ctx_meter_ink`):
/// `ATTENTION` from `CTX_METER_WARN`, `BLOCKED` from `CTX_METER_FULL`.
pub const CTX_METER_WARN: f32 = 0.60;
pub const CTX_METER_FULL: f32 = 0.85;
/// Under the working line, above Solo's box: 15 (off the scale: the
/// mockup's, the transcript line's half-leading over the box's 14px
/// inset).
pub const COMPOSER_WORKING_GAP_SOLO: f32 = 15.0;
/// The provider's logomark in a status segment or a picker section.
pub const STATUS_LOGO: f32 = 11.0;
/// A logomark set inline in a status segment's line (the prototype's
/// `svg.logo`, `vertical-align: -1px`) has its foot 1px under the
/// baseline: its top 4px down a 13/20 line, half a pixel over the centre a
/// flex row gives it. As a negative top margin on the centred mark it
/// lifts the mark by half its value: onto that top.
pub const STATUS_LOGO_FOOT: f32 = 1.0;
/// Multiline drafts, controls and queued prompts share a bounded part of
/// the Pane, keeping most of its height available to the conversation.
pub const COMPOSER_MAX_PANE_FRACTION: f32 = 0.45;
/// Between the shelf (pending files, background chips) and the box.
pub const SHELF_GAP: f32 = SPACE_1;
/// The caret's bar: 2px wide (off the space scale: a caret is a hairline
/// you can find), the row high.
pub const CARET_W: f32 = 2.0;
/// One selection colour app-wide: the Composer paints the transcript's
/// native selection wash under its selected runs.
pub const COMPOSER_SELECTION: u32 = TEXT_SELECTION_WASH;
/// An `@`-mention the operator picked: a path, so `LINK_INK` on no
/// ground — the terminal's own colour for a path.
pub const MENTION_INK: u32 = LINK_INK;
/// The Context tab's blocks (`pane::context_usage`): between one window's
/// block and the next, and inside one block; the blocks stand twice as far
/// apart as their own lines.
pub const USAGE_CARD_GAP: f32 = SPACE_3;
pub const USAGE_CARD_ROW_GAP: f32 = SPACE_1_5;
pub const USAGE_CARD_BAR_H: f32 = 4.0;
/// Where a usage reading turns from neutral to ATTENTION (80%, rule
/// 2.6.6) — a fraction of the window, not a count — on the status line's
/// account windows and the card alike.
pub const USAGE_TIGHT: f32 = 0.80;
/// The session-controls card: permission modes, MCP servers and background
/// tasks as sections of menu rows, wide enough for a server's name beside
/// its state and two quiet actions (`needs sign-in`, `Sign in`,
/// `Reconnect`).
pub const SESSION_CARD_W: f32 = 416.0;
/// One row of the usage card's legend, its colour square, and the share
/// column the percentages right-align in.
pub const USAGE_LEGEND_ROW_H: f32 = 20.0;
pub const USAGE_SWATCH: f32 = 8.0;
pub const USAGE_SHARE_W: f32 = 44.0;
/// What fills the context window, one ink per category in the stacked bar
/// and its legend: the terminal palette, the operator's own words first
/// (blue, as the retired accent had them: a category, not a state).
pub const CTX_MESSAGES: u32 = BLUE;
pub const CTX_TOOLS: u32 = ORANGE;
pub const CTX_MCP: u32 = GREEN;
pub const CTX_SKILLS: u32 = YELLOW;
pub const CTX_PROMPT: u32 = MAGENTA;
pub const CTX_MEMORY: u32 = CYAN;
pub const CTX_BUFFER: u32 = TEXT_DIM;
pub const CTX_FREE: u32 = LINE2;
pub const CTX_DEFERRED: u32 = TEXT_FAINT;
pub const CTX_CYCLE: [u32; 4] = [BLUE, RED, 0x8fb2b8, 0xb5b48e];
/// 28px — the air a fitted image keeps from its reader slot's edges (the
/// image viewer): the reader's own text inset. Zoomed past the slot, the
/// picture pans right to them.
pub const IMAGE_INSET: f32 = TX_INSET_SOLO;
/// The scales ⌘= and ⌘- step an image through (1 is actual size: an image
/// pixel to a point). Below the fit there is no stop: ⌘- lands on the fit.
pub const IMAGE_ZOOM_STOPS: [f32; 11] = [
    0.25,
    1.0 / 3.0,
    0.5,
    2.0 / 3.0,
    1.0,
    1.5,
    2.0,
    3.0,
    4.0,
    6.0,
    8.0,
];
/// 8× — the closest an image zooms, by key or by pinch.
pub const IMAGE_ZOOM_MAX: f32 = IMAGE_ZOOM_STOPS[IMAGE_ZOOM_STOPS.len() - 1];
/// 3px — how far a press on an image travels before it is a pan rather
/// than a click.
pub const IMAGE_DRAG_SLOP: f32 = 3.0;
/// A background task on the shelf above the Composer (a shelf chip): the
/// still `RUNNING` dot, the task cut at 32 cells, and a quiet `×` that
/// stops it.
pub const BG_CHIP_MAX_W: f32 = 32.0 * CH;
/// A pending file on the shelf (a shelf chip): the image mark (or a 12px
/// thumbnail) and the name in `LINK_INK`, cut at 32 cells, then `×`.
pub const ATTACH_CHIP_H: f32 = SHELF_CHIP_H;
pub const ATTACH_CHIP_MAX_W: f32 = 32.0 * CH;
pub const ATTACH_THUMB: f32 = 12.0;
/// The model picker hangs off the model chip: its left edge 8px left of
/// the chip's (so its section logo sits left of the chip's logo), its foot
/// 5px over the chip.
pub const MODEL_PICKER_NUDGE: f32 = 8.0;
pub const MODEL_PICKER_GAP: f32 = 5.0;
/// The Solo Composer's box: `COMPOSER_INSET` (14, off the scale: the
/// mockup's inset, between `SPACE_3` and `SPACE_4`) from the card's sides
/// and foot.
pub const COMPOSER_INSET: f32 = 14.0;
/// A shelf chip: 24 high.
pub const SHELF_CHIP_H: f32 = SPACE_6;
/// The queued row: 34 high (the mockup's; a 20px line in 7px of air), its
/// words `QUEUED_PAD_L` in, its close button `QUEUED_PAD_R` from the end.
pub const QUEUED_ROW_H: f32 = 34.0;
pub const QUEUED_PAD_L: f32 = COMPOSER_INPUT_PAD_X;
pub const QUEUED_PAD_R: f32 = SPACE_2;
/// Between the queued row's parts: 10 (the mockup's).
pub const QUEUED_GAP: f32 = 10.0;
/// The queued row's close button, and its `close` icon.
pub const QUEUED_CLOSE: f32 = SPACE_6;
pub const QUEUED_CLOSE_ICON: f32 = SPACE_3;
/// The input's padding: 14 above and at the sides (`COMPOSER_INSET`'s
/// measure), 8 under.
pub const COMPOSER_INPUT_PAD_T: f32 = COMPOSER_INSET;
pub const COMPOSER_INPUT_PAD_X: f32 = COMPOSER_INSET;
pub const COMPOSER_INPUT_PAD_B: f32 = SPACE_2;
/// The toolbar row: 42 high (a 28px chip in 7px of air), 8 in from the
/// box's sides, its controls 2 apart.
pub const COMPOSER_TOOLBAR_H: f32 = 42.0;
pub const COMPOSER_TOOLBAR_PAD_X: f32 = SPACE_2;
pub const COMPOSER_TOOLBAR_GAP: f32 = SPACE_0_5;
/// The `+` attach button (30, off the scale: the mockup's) and its icon.
pub const COMPOSER_ATTACH: f32 = 30.0;
pub const COMPOSER_ATTACH_ICON: f32 = SPACE_4;
/// A toolbar chip: 28 high, 9 in at each side (the mockup's), its parts 6
/// apart; its chevron 11, the mode's shield 14, the provider's logo 13.
pub const TOOL_CHIP_H: f32 = 28.0;
pub const TOOL_CHIP_PAD_X: f32 = 9.0;
pub const TOOL_CHIP_GAP: f32 = SPACE_1_5;
pub const TOOL_CHIP_CHEVRON: f32 = 11.0;
pub const TOOL_CHIP_ICON: f32 = 14.0;
pub const TOOL_CHIP_LOGO: f32 = 13.0;
/// The send circle (28) and its arrow (14), 6 past the last chip.
pub const COMPOSER_SEND: f32 = 28.0;
pub const COMPOSER_SEND_ICON: f32 = 14.0;
pub const COMPOSER_SEND_GAP: f32 = SPACE_1_5;
// (end WP-D) — append above this line only

// ======================================== WP-E · menus, popovers, sheets, notifications
// Owner: WP-E (menus, popovers, Settings, the Project editor, notifications.)
// Edit values and append tokens only inside this section.

/// **One float grammar** (SPEC §5.8, `menu.rs` draws it): every menu,
/// picker, popover, card, toast and sheet is `components::floating_surface`
/// — `FLOAT_GROUND`, radius `R_BLOCK`, a 1px `FLOAT_EDGE` ring
/// (`HAIRLINE_STRONG`), the one float shadow — the palette's shell at a
/// smaller scale (the shell head names its metrics).
///
/// - **A head** (`menu::head`): one row, the surface's title in
///   `TEXT_STRONG` at `W_LABEL` at the left, its command or verb in
///   `TEXT_DIM` at the right (`Model  /model`).
/// - **A section** (`menu::section`): a `FLOAT_SECTION_H` heading in
///   12/500 `TEXT_DIM`, `FLOAT_SECTION_GAP` above it, a provider's
///   logomark leading in its brand colour.
/// - **A row** (`menu::row`): `FLOAT_ROW_H`, inset `FLOAT_PAD`, radius
///   `R_MENU_ROW`; a leading mark, the name in `TEXT`, its description
///   `TEXT_DIM` after it, a key in `TEXT_DIM` hard right, a `✓` on the
///   standing choice. The cursor row is `FLOAT_SEL` with its name
///   `TEXT_STRONG`, a row under the pointer `FLOAT_HOVER`.
/// - **A footer** (`menu::footer`): a `FLOAT_RULE` hairline, then one row
///   of key hints in 12 `TEXT_DIM` (its `·` too): `↑↓ select · ⏎ open ·
///   esc`.
/// - **An inverse token** (`menu::token`): the chosen one of a short ladder
///   (an effort, a two-way setting) on `PRIMARY` in `ON_PRIMARY`; the rest
///   `TEXT_DIM`, `FLOAT_HOVER` under the pointer.
pub const FLOAT_DETAIL_GAP: f32 = MENU_ROW_GAP;
pub const FLOAT_SECTION_GAP: f32 = SPACE_1;
pub const FLOAT_FOOT_H: f32 = FLOAT_ROW_H + 1.0;
/// A ladder's step: 22 high (the bar tab's), radius `R_SMALL`.
pub const TOKEN_H: f32 = 22.0;
pub const TOKEN_PAD_X: f32 = SPACE_2;
/// The `/` and `@` menus: at most the palette's width, hung over the
/// Composer.
pub const SLASH_MENU_W: f32 = PALETTE_W;
/// The model picker: a model's name, its description and the `✓` on one
/// row.
pub const MODEL_PICKER_W: f32 = 520.0;
/// A form field or a sheet's text button: one row in a cell of padding,
/// inside a 1px edge at `R_CONTROL` (a field's the `FIELD_RING` hairline,
/// `CONTROL_FOCUS` while it holds the caret).
pub const FORM_CONTROL_H: f32 = ROW + 2.0 * SPACE_1 + 2.0;
pub const FORM_FIELD_PAD_X: f32 = CH;
pub const FORM_BUTTON_PAD_X: f32 = CH;
/// A tooltip: one float row (`FLOAT_ROW_H`), its text `FLOAT_PAD_X` in,
/// wrapping past `TOOLTIP_MAX_W`. It is how a truncated label keeps its
/// full value reachable.
pub const TOOLTIP_PAD_X: f32 = FLOAT_PAD_X;
pub const TOOLTIP_PAD_Y: f32 = (FLOAT_ROW_H - LH_UI) / 2.0;
pub const TOOLTIP_MAX_W: f32 = 320.0;
/// The kit's own toast stack (unused by the Needs-you view, kept for the kit's
/// margins in `init_components`): this far up from the window's foot.
pub const TOAST_ABOVE_COMPOSER: f32 =
    GRID_PAD + 1.0 + 2.0 * COMPOSER_PAD_Y + COMPOSER_ROW_H + COMPOSER_STATUS_PAD_B + HALF_ROW;
/// **A toast**: a float `TOAST_W` wide, its right edge
/// `TOAST_BOX_RIGHT` in from the window's, its foot `TOAST_BOTTOM` over the
/// bottom bar. One row of head (`◆ needs you · <title>`, `⌘D`), one body
/// line, then the quick answers: `TOKEN_H` pills (`menu::token`'s size),
/// `TOAST_QUICK_GAP` over the float's edge.
pub const TOAST_BOX_RIGHT: f32 = SPACE_4;
pub const TOAST_BOTTOM: f32 = SPACE_4;
pub const TOAST_QUICK_GAP: f32 = SPACE_2;
/// **The palette** (command search, ⌘K; SPEC §5.8, `command-search.png`):
/// a 640px dialog centred on the board, its head `PALETTE_TOP` under the
/// board's top, over the veil (`VEIL`, the board only), on
/// `PALETTE_GROUND`, radius `R_PALETTE`, a 1px `HAIRLINE_STRONG` ring and
/// the deep shadow (`SHADOW_PALETTE_*`). Top to bottom:
///
/// - the input row, `PALETTE_INPUT_H` over a `HAIRLINE`: the `search`
///   icon (`PALETTE_SEARCH_ICON`), the query in `FS_PALETTE`, `esc` (or
///   the status filter's word) at the right, `PALETTE_PAD_X` in from
///   either edge, `PALETTE_GAP` between;
/// - the list, `PALETTE_LIST_PAD` in (`PALETTE_LIST_PAD_B` at its foot),
///   scrolling past `PALETTE_MAX_H`: section headings `PALETTE_SECTION_H`
///   tall, their 12/500 word on the foot less `PALETTE_SECTION_PAD_B`, every
///   heading after the first `PALETTE_SECTION_GAP` lower; rows
///   `PALETTE_ROW_H`, radius `R_CONTROL`, `PALETTE_ROW_PAD_X` in, their
///   parts `PALETTE_GAP` apart — the `PALETTE_MARK_W` mark column, the
///   name, the context and chord at the right, then the
///   `PALETTE_ENTER_ICON` enter mark's column;
/// - the footer, `PALETTE_FOOT_H` under a `HAIRLINE`, `PALETTE_PAD_X` in.
///
/// The shortcuts sheet takes the same width, input row and veil.
pub const PALETTE_W: f32 = 640.0;
pub const PALETTE_TOP: f32 = 56.0;
/// The dialog's ground: `FRAME`'s `#222`, not `FLOAT`. The mockup paints
/// it so, and on `FLOAT` (`#2a2a2c`) the `SELECTED` cursor row would sit
/// a bare 4 levels over its ground and all but vanish (a deliberate
/// departure from SPEC §5.8's `FLOAT ground`). Opaque on every platform,
/// as every float is (FL-15).
pub const PALETTE_GROUND: Paint = Paint(solid(FRAME));
/// The palette's rows on that ground: the frame's own steps, opaque —
/// `RAISED` under the pointer, `SELECTED` the cursor (the mockup's
/// `#2e2e2e`). The float inks (`FLOAT_HOVER`, `FLOAT_SEL`) step over
/// `FLOAT` instead.
pub const PALETTE_HOVER: Paint = Paint(solid(RAISED));
pub const PALETTE_SEL: Paint = Paint(solid(SELECTED));
pub const PALETTE_INPUT_H: f32 = 50.0;
pub const PALETTE_PAD_X: f32 = SPACE_4;
/// 10 — between a row's parts, and the input row's (off the space scale,
/// the mockup's flex gap).
pub const PALETTE_GAP: f32 = 10.0;
pub const PALETTE_SEARCH_ICON: f32 = 16.0;
pub const PALETTE_LIST_PAD: f32 = SPACE_1_5;
pub const PALETTE_LIST_PAD_B: f32 = SPACE_2;
/// 32: a 28px heading and the 4px under its word (the mockup's content
/// box, `height: 28px; padding-bottom: 4px`); a later heading stands
/// `PALETTE_SECTION_GAP` taller.
pub const PALETTE_SECTION_H: f32 = 32.0;
pub const PALETTE_SECTION_PAD_B: f32 = SPACE_1;
pub const PALETTE_SECTION_GAP: f32 = SPACE_1;
pub const PALETTE_ROW_H: f32 = 36.0;
/// 10 — a row's inset, the name column on the input's text edge.
pub const PALETTE_ROW_PAD_X: f32 = 10.0;
pub const PALETTE_MARK_W: f32 = 14.0;
pub const PALETTE_MARK_ICON: f32 = 14.0;
pub const PALETTE_ENTER_ICON: f32 = 13.0;
pub const PALETTE_FOOT_H: f32 = 36.0;
pub const PALETTE_MAX_H: f32 = 12.0 * PALETTE_ROW_H;
/// **The find bar** (⌘F): a one-row float `FIND_W` wide, hung
/// `FLOAT_OFFSET` under the top right of the text it searches — the
/// `search` icon (`FIND_ICON`), the query, the count.
pub const FIND_W: f32 = 320.0;
pub const FIND_ICON: f32 = 14.0;
/// **FL-15 · frosted floats: blocked in gpui-pre-macos 0.3.3, so every
/// float stays opaque.** The prototype frosts its floats (`.glass .float`:
/// a 30px backdrop blur under `rgba(44,44,47,.74)`). gpui blurs no element,
/// so a float could only frost what is behind it as a window of its own
/// (`WindowKind::PopUp`, `WindowBackgroundAppearance::Blurred`). That
/// window cannot be the prototype's float, for three reasons, each in the
/// gpui-pre-macos 0.3.3 source:
///
/// 1. **It is always a titled window.** `MacWindow::open` builds the style
///    mask as `NSTitledWindowMask | NSFullSizeContentViewWindowMask` when no
///    titlebar is asked for (`src/window.rs:918-937`), and PopUp only adds
///    `NSWindowStyleMaskNonactivatingPanel` (`src/window.rs:946`). AppKit
///    gives a titled window rounded corners and its own window shadow, and
///    `WindowOptions` offers no borderless mask, corner radius or shadow
///    switch (`gpui-pre-0.3.3/src/platform.rs:1948-2017`; its
///    `window_decorations` is X11/Wayland only, `:2008-2010`). A float has
///    its own radius (rule 2) and casts the one `0 6px 14px` shadow;
///    drawn inside the window instead, that shadow's margin would be
///    frosted with the rest of the rect, since the blur view fills the
///    whole content view (`set_background_appearance`,
///    `src/window.rs:1659-1692`).
/// 2. **It takes the keyboard when clicked.** Both window classes answer
///    `canBecomeKeyWindow` with YES (`build_window_class`,
///    `src/window.rs:376`, used for the panel class at `:134`); the
///    non-activating mask keeps the app from activating, not the panel from
///    becoming key. A palette or picker clicked with the pointer would pull
///    the keyboard out of the main window, whose key table (the Composer,
///    the Decision keys, the palette's own) is the only one the floats run
///    on.
/// 3. **No anchored native popup exists.** `MacPlatform::open_window`
///    rejects `WindowKind::AnchoredPopup` with `PopupNotSupportedError`
///    (`src/platform.rs:657-661`), so a PopUp is placed by hand in screen
///    space and follows no move, resize, Space or fullscreen change of its
///    owner.
///
/// The verdict: floats render in-window and opaque (`float::hang`), on the
/// opaque float inks below (`FLOAT_GROUND` `#2a2a2c`, `FLOAT_SEL` and
/// `FLOAT_HOVER` its own steps, `FLOAT_SELECTED` and `FLOAT_RAISED`; the
/// edge and rule are hairlines), never a glass overlay over an opaque
/// ground, and `FLOAT_GLASS` stays opaque. Flip `FLOAT_FROSTED` when a
/// gpui release lifts the three blocks; the inks then follow the glass
/// values.
pub const FLOAT_FROSTED: bool = false;
const fn float_ink(glass: u32, opaque: u32) -> Paint {
    if FLOAT_FROSTED {
        Paint::pick(glass, solid(opaque))
    } else {
        Paint(solid(opaque))
    }
}
/// A float's ground, edge, inner rule (under a head, over a footer), the
/// cursor row and a row under the pointer (FL-14).
pub const FLOAT_GROUND: Paint = float_ink(FLOAT_GLASS, FLOAT);
pub const FLOAT_EDGE: Paint = paint::HAIRLINE_STRONG;
pub const FLOAT_RULE: Paint = paint::HAIRLINE;
pub const FLOAT_SEL: Paint = float_ink(SELECTED_GLASS, FLOAT_SELECTED);
pub const FLOAT_HOVER: Paint = float_ink(RAISED_GLASS, FLOAT_RAISED);
/// A quick answer's pill on a float (a toast's buttons), stepping up to
/// `FLOAT_SEL` under the pointer.
pub const FLOAT_BAND2: Paint = float_ink(RAISED_GLASS, FLOAT_RAISED);
/// A sheet (Settings, the Project editor, the image preview) is a float at
/// sheet size: its head two rows (the title and the one close control over
/// a `paint::LINE`), its body two cells in, a footer under a `paint::LINE`.
pub const MODAL_HEAD_H: f32 = 2.0 * ROW;
pub const MODAL_PAD: f32 = 2.0 * CH;
pub const MODAL_GAP: f32 = HALF_ROW;
/// Editors leave an even breathing edge while making room for a scrolling
/// form at short desktop heights.
pub const MODAL_VIEWPORT_FRACTION: f32 = 0.92;
/// A kit-hosted choice menu (`components::ChoiceMenu`: model, effort, mode,
/// subagent overflow): wide enough for a model name beside its check.
pub const CHOICE_MENU_MIN_W: f32 = 240.0;
pub const CHOICE_MENU_MAX_W: f32 = 320.0;
/// About 48 characters — how much of a long directory a menu row keeps,
/// cut at its head behind `…/` (the tail names the place).
pub const MENU_PATH_TAIL: usize = 48;
// ---------------------------------------------------------- Settings sheet
//
// **The Settings sheet** (`prefs.rs`) is a float at sheet size: the
// float's shell (radius `R_BLOCK`, the `FLOAT_EDGE` ring), Geist on the
// space scale, rows and hairlines, no cards and no iOS controls.
//
// - **Sidebar** (`SETTINGS_SIDEBAR_W`): the search line on top (the
//   `search` icon, then the field), a `HAIRLINE` under it, then one
//   `SETTINGS_NAV_ROW_H` row per page, inset `FLOAT_PAD` like a menu's,
//   radius `R_MENU_ROW` — `FLOAT_SEL` and `TEXT_STRONG` on the page shown,
//   `TEXT` on the rest — and `About` at the foot. ↑/↓ step pages while the
//   sheet holds focus.
// - **Content:** the page title in `TEXT_STRONG` `W_STRONG`, then groups
//   `SETTINGS_GROUP_GAP` apart: a group's label is a `TEXT_DIM` row (a
//   provider's led by its logomark), its rows split by rules.
// - **Row:** the label (`TEXT`) over an optional hint (`TEXT_DIM`) at the
//   left; the control right and centred.
// - **Controls** are words on the row: an on/off setting is `on` in
//   `TEXT_STRONG` or `off` in `TEXT_DIM`; a two-way choice is a pair of
//   tokens, the chosen one inverse (`menu::token`); a longer list is the
//   value and a chevron that opens a menu; a size is `‹ 14px ›`. Each is a
//   tab stop with the one focus outline, `paint::RAISED` under the pointer.
// - **About** is a list of facts: the key at the left, the value at the
//   right in `TEXT_DIM`. A path copies on click.
// - **Search** filters every page at once: matching rows keep their groups,
//   each labelled `Page · Group`; pages with no match read `TEXT_DIM`.
pub const SETTINGS_W: f32 = 760.0;
pub const SETTINGS_H: f32 = 448.0;
pub const SETTINGS_SIDEBAR_W: f32 = 188.0;
pub const SETTINGS_NAV_ROW_H: f32 = FLOAT_ROW_H;
/// A setting row: one row in a quarter row of air each side, or the label
/// and its hint (two rows) in the same.
pub const SETTINGS_ROW_H: f32 = ROW + HALF_ROW;
pub const SETTINGS_ROW_HINT_H: f32 = 2.0 * ROW + HALF_ROW;
pub const SETTINGS_ROW_PAD_X: f32 = SPACE_2;
/// Between groups; a group's label sits directly on its rows.
pub const SETTINGS_GROUP_GAP: f32 = ROW;
pub const SETTINGS_TITLE_GAP: f32 = HALF_ROW;
/// Every control in a row is one row high.
pub const SETTINGS_CONTROL_H: f32 = ROW;
/// The text-size stepper's value slot: `24px` never moves the `›`.
pub const SETTINGS_STEPPER_VALUE_W: f32 = 48.0;
/// A chooser's widest value before it truncates (a model's name).
pub const SETTINGS_MENU_MAX_W: f32 = 220.0;
/// An About key's column, so every value starts on one edge.
pub const SETTINGS_FACT_KEY_W: f32 = 110.0;
/// A typed field on a Settings row (Machines' ssh target).
pub const SETTINGS_FIELD_W: f32 = 188.0;
// (end WP-E) — append above this line only

// ======================================== WP-F · decisions and subagents
// Owner: WP-F (decision.rs, subagents.rs, the Decision card and keycaps.)
// Edit values and append tokens only inside this section.

/// The Decision (approvals, questions, forms, links — Main's and a
/// Subagent's alike, rule 2.8) is the transcript's tail row, drawn as a box
/// (SPEC §5.9 item 3) across the transcript column, Solo's and a Group
/// Pane's alike: `DECISION` ground at `R_BLOCK`, an inset 1px
/// `DECISION_RING`, padded `DECISION_PAD`, its sections `DECISION_GAP`
/// apart, in Geist. The head (`DECISION_HEAD_ICON` `shield` in
/// `ATTENTION`, a question's `?`; the lead 13 at `W_LABEL` `TEXT_STRONG`;
/// the context at the right in 12 `TEXT_DIM`), the command well (`SUNKEN`
/// at `R_TAB`, Geist Mono 12.5, a `TEXT_DIM` `$ `), then the options:
/// `DECISION_OPTION_H` rows at `R_TAB`, the key in Geist Mono 12 `TEXT_DIM`,
/// the label in Geist 13, the cursor's row `SELECTED` and `TEXT_STRONG`.
/// The hint line (`↑↓ select · 1–3 pick · esc deny · tab amend`, 12
/// `TEXT_DIM`) sits `DECISION_HINT_GAP` under the box, on the Pane that
/// holds the keyboard only. Head 20 · gap 10 · well 32 · gap 10 · options
/// 3 × 30, inside 12 of padding.
///
/// Where a card still docks over the transcript's foot (the requests
/// overlay) it lies on the plane, `DECISION_PAD_Y` inside.
pub const DECISION_PAD_Y: f32 = HALF_ROW;
pub const DECISION_PAD: f32 = SPACE_3;
pub const DECISION_GAP: f32 = 10.0;
pub const DECISION_HINT_GAP: f32 = SPACE_2;
pub const DECISION_HEAD_ICON: f32 = 14.0;
pub const DECISION_OPTION_H: f32 = 30.0;
pub const DECISION_OPTION_PAD_X: f32 = 10.0;
/// The key's column to its label.
pub const DECISION_OPTION_GAP: f32 = 10.0;
/// Option rows sit flush; a description adds a line under the label.
pub const DECISION_ROW_GAP: f32 = 0.0;
/// A question's text to its rows, and one question to the next.
pub const DECISION_QUESTION_GAP: f32 = HALF_ROW;
pub const DECISION_QUESTIONS_GAP: f32 = ROW;
/// The command well: 10 in, 6 above and below. It scrolls past 160px, and
/// when the Pane is short it is the one section that shrinks, never below
/// one line (`DECISION_WELL_MIN_H`).
pub const DECISION_WELL_PAD_X: f32 = 10.0;
pub const DECISION_WELL_PAD_Y: f32 = SPACE_1_5;
pub const DECISION_WELL_MIN_H: f32 = LH_UI + 2.0 * DECISION_WELL_PAD_Y;
pub const DECISION_INPUT_MAX_H: f32 = 160.0;
/// A question body's scroll cap inside the card (head and footer stay
/// pinned); container-relative, never a window fraction.
pub const DECISION_BODY_MAX_H: f32 = 320.0;
/// Below a 360px Pane the body caps at two described option rows and
/// scrolls, so the head and the answer row always stay in reach.
pub const DECISION_SHORT_PANE_H: f32 = 360.0;
pub const DECISION_SHORT_BODY_MAX_H: f32 = 2.0 * (DECISION_OPTION_H + LH_UI) + DECISION_ROW_GAP;
/// The scroll gutter a body keeps free for its thumb.
pub const DECISION_SCROLL_GUTTER: f32 = SPACE_1;
/// A question's own answer (rule 2.8.6) is not a second field: it is one
/// bare input line on the option labels' column and in their face (Geist,
/// SPEC §5.9), `LH_UI` high, placeholder `Or type your own answer…` in
/// `TEXT_DIM`; the digit one past the last option arms it.
pub const QUESTION_OTHER_H: f32 = LH_UI;
/// The primary's `↵`: `ON_PRIMARY` at 70%, so the key reads under its label
/// on the primary fill.
pub const SEND_KEY_INK: u32 = 0x111214b3;

/// The Solo card's tab strip (SPEC §5.3): one `SUBJECT_STRIP_H` (38) row
/// at the card's top — under the Group head in a board cell — padded
/// `SUBJECT_STRIP_PAD_X` (6) at both ends, its tabs `SUBJECT_TAB_GAP` (2)
/// apart, closed by a `HAIRLINE` the body clips at. A tab is
/// `SUBJECT_TAB_H` (28) high at `R_TAB`, `FS_LABEL` Geist: the selected one
/// on `TAB_ACTIVE` in `TEXT_STRONG`, the others bare in `TEXT_MUTED`.
/// `Main` leads with the `terminal` icon (`SUBJECT_TAB_ICON`, 13) and pads
/// `SUBJECT_TAB_PAD_L` / `SUBJECT_MAIN_PAD_R` (10 / 8); a live Subagent's
/// tab pads 10 both sides and leads with its mark in a fixed
/// `SUBJECT_MARK_W` slot (Geist Mono 12's cell: the braille spinner's), so
/// a state change never resizes it. Labels ellipsise at
/// `SUBJECT_LABEL_MAX_W` (200). Icon or mark to label: `SUBJECT_TAB_INNER_GAP`
/// (8). The `N done ▾` menu button pads 10 / 8, its parts
/// `SUBJECT_MENU_GAP` (6) apart, after a 1×16 `HAIRLINE` separator with
/// `SUBJECT_SEPARATOR_MX` (6) either side. Off the scale: the mockups' tabs.
pub const SUBJECT_STRIP_H: f32 = 38.0;
pub const SUBJECT_STRIP_PAD_X: f32 = SPACE_1_5;
pub const SUBJECT_TAB_GAP: f32 = SPACE_0_5;
pub const SUBJECT_TAB_H: f32 = 28.0;
pub const SUBJECT_TAB_PAD_L: f32 = 10.0;
pub const SUBJECT_MAIN_PAD_R: f32 = SPACE_2;
pub const SUBJECT_TAB_ICON: f32 = 13.0;
pub const SUBJECT_MARK_W: f32 = 0.6 * FS_META;
pub const SUBJECT_TAB_INNER_GAP: f32 = SPACE_2;
pub const SUBJECT_LABEL_MAX_W: f32 = 200.0;
pub const SUBJECT_MENU_GAP: f32 = SPACE_1_5;
pub const SUBJECT_MENU_CHEVRON: f32 = 11.0;
pub const SUBJECT_SEPARATOR_H: f32 = SPACE_4;
pub const SUBJECT_SEPARATOR_MX: f32 = SPACE_1_5;
/// A Subagent tab's `Back to Main` row in the Composer's place (SPEC
/// §5.3): `SUBJECT_STRIP_H` high, the coverage words in `TEXT_DIM` on the
/// Composer's inset.
pub const CHILD_FOOTER_H: f32 = SUBJECT_STRIP_H;
// (end WP-F) — append above this line only

// ======================================== WP-G · nav
// Owner: WP-G (nav.rs and its cockpit wiring.)
// Edit values and append tokens only inside this section.
//
// **The nav is a list on the frame** (SPEC §5.1, `mockups/solo.png`). The
// column is `paint::FRAME` from the window's top, `NAV_COL_PAD` in on its
// sides and foot, with no seam: the card beside it does the separating.
// Top to bottom:
//
//   head      ● ● ●                       [▯]      46, in the head row
//   views     ⌕  Search                    ⌘K      30 each, then 14
//             ⊡  Needs you                 (3)
//             ▤  Parked                     3
//   heading   Projects                      +      26
//   project   ▭  agenticos                ↑715     30
//   thread    ⠹  Ferrite idle memory   +4 −1      44, two lines
//                scheduled-work-crash · 5m22s
//   group        › WORK              1 failed · 4  30
//   foot      Ferrite 0.5                   ⚙      36
//
// Chrome is Geist; numbers from the terminal world (diffstats, spinners)
// are Geist Mono. Rows are rounded (`R_CONTROL`), wear `paint::RAISED`
// under the pointer (`MOTION_NAV_HOVER_MS`) and `paint::SELECTED` with an
// inset `ROW_RING` when they stand for what the board shows. Colour is
// state only: the mark (§6), a blocked or failed row's line 2, a Group's
// roll-up word, the needs-you pill.
//
// **Titles truncate against a definite width.** gpui only measures an
// ellipsis against a width it knows on the line's first measure, so every
// truncating box is pinned to what its row leaves it, measured from the
// face's own advances (`nav::Measure`).
//
// **Folding (cmd-B).** The column rides its width from `NAV_WIDTH` to
// the rail (`NAV_FOLDED_W`) over `motion::RESIZE` (200ms) while its
// content fades independently (`MOTION_NAV_FADE_MS`, CSS `ease`: 1 → 0
// folding, 0 → 1 opening). The rail is one `NAV_RAIL_CELL` per open
// Thread, its mark and its ⌘ digit.

/// 8px — the column's padding at its sides and foot (`padding: 0 8 8`).
pub const NAV_COL_PAD: f32 = SPACE_2;
/// 10px · 4px — the head row's padding inside the column, left and right:
/// the traffic lights' spacer, then the sidebar toggle.
pub const NAV_HEAD_PAD_L: f32 = 10.0;
pub const NAV_HEAD_PAD_R: f32 = SPACE_1;
/// 10px — every row's inline padding inside the column; the icons and the
/// labels of views, headings and Projects start here.
pub const NAV_ROW_PAD_X: f32 = 10.0;
/// 30px — a one-line row: a view, a Project heading, a Group, a shown
/// Group's member, and a Thread row with nothing for line 2.
pub const NAV_ROW_H: f32 = 30.0;
/// 44px — a two-line Thread row: its title, then its branch or reason.
pub const NAV_ROW2_H: f32 = 44.0;
/// 20px — one line of a row (line 1, line 2, a task's extra lines).
pub const NAV_LINE: f32 = ROW;
/// 15px — a view's or a Project's icon, `NAV_ICON_GAP` before its label.
pub const NAV_ICON: f32 = 15.0;
pub const NAV_ICON_GAP: f32 = 10.0;
/// 14px — under the views, before the tree.
pub const NAV_VIEWS_GAP: f32 = 14.0;
/// 26px — a section heading (`Projects`, `Groups`, `Recent`): 12/500
/// `TEXT_DIM`, padding 0 6 0 10, its 22px `+` at the right.
pub const NAV_HEADING_H: f32 = 26.0;
pub const NAV_HEADING_PAD_R: f32 = SPACE_1_5;
pub const NAV_HEADING_BUTTON: f32 = 22.0;
pub const NAV_HEADING_GLYPH: f32 = 13.0;
/// 16px — the sidebar toggle's and the gear's glyph, in an `ICON_BUTTON`;
/// the panel toggle, the sidebar toggle's twin, shares it.
pub const NAV_TOGGLE_GLYPH: f32 = 16.0;
/// 35px — where a Thread row's title starts (`padding: 0 10 0 35`); its
/// mark sits in the 10–24 column before it.
pub const NAV_TITLE_X: f32 = 35.0;
/// 54px — where a shown Group's member title starts at the tree's root;
/// its mark at 35. One level in, both move `NAV_TREE_INDENT` with it.
pub const NAV_MEMBER_X: f32 = 54.0;
/// 14px — one level of the tree (§5.1.4): everything inside a Project
/// (its Threads, its Group rows and their members) sits one level under
/// its heading, and a Group's members one more under the Group. Only a
/// row's content moves in; its hover and selected fills keep the full
/// width.
pub const NAV_TREE_INDENT: f32 = 14.0;
/// A tree guide: the strong hairline (white at 10%), down a Project's (or
/// a Group's) children. Structure, not state, so a hairline (rule 2).
pub const NAV_GUIDE: Paint = paint::HAIRLINE_STRONG;
/// 1px — a guide's width: a hairline's.
pub const NAV_GUIDE_W: f32 = 1.0;
/// 8px — how far short of its last row's foot a guide stops, so the tree
/// reads as ending at that row rather than running into the next heading.
pub const NAV_GUIDE_TAIL: f32 = SPACE_2;
/// 17px — a Project's guide, under the middle of its folder icon.
pub const NAV_PROJECT_GUIDE_X: f32 = NAV_ROW_PAD_X + NAV_ICON / 2.0 - NAV_GUIDE_W / 2.0;
/// 17px — a Group's guide, under the middle of its chevron (at 17, so on
/// the whole pixel layout snaps it to), at the tree's root; a Group one
/// level in has it `NAV_TREE_INDENT` further.
pub const NAV_GROUP_GUIDE_X: f32 = NAV_CHEVRON_X + NAV_CHEVRON / 2.0;
/// 8px — between a row's title and what sits at its right.
pub const NAV_WORD_GAP: f32 = SPACE_2;
/// 6px — between two Project sections.
pub const NAV_SECTION_GAP: f32 = SPACE_1_5;
/// A Thread row's marks (§6): 7px dots (6 on a member row) with the
/// blocked one's 3px halo, 14px icons (13 on a member row), the spinner's
/// Geist Mono 12.
pub const NAV_DOT: f32 = 7.0;
pub const NAV_DOT_SMALL: f32 = 6.0;
pub const NAV_HALO: f32 = 3.0;
/// The blocked dot's halo box: the dot and `NAV_HALO` out on each side.
pub fn nav_halo_box(dot: f32) -> f32 {
    dot + 2.0 * NAV_HALO
}
/// The middle of a two-line row's line 1 (its two lines centred in the
/// row): where its mark centres, on the title's own middle (§5.1.5
/// "centred on line 1"; the mockups' marks sit 3–5px under it, on the
/// baseline, which the eye reads as dropped).
pub const NAV_LINE1_MID: f32 = (NAV_ROW2_H - 2.0 * NAV_LINE) / 2.0 + NAV_LINE / 2.0;
/// Where a mark sits, `(x, y)` from its mark column's left and its row's
/// top: centred on line 1 of a two-line row (the spinner's line box on
/// line 1's own), centred on a one-line row (`_LINE`) and on a shown
/// Group's member (`_SMALL`). The x are the mockups' (measured, so off the
/// scale). The blocked dot sits where a dot does, its halo `NAV_HALO` out
/// round it.
pub const NAV_SPINNER_AT: (f32, f32) = (2.0, NAV_LINE1_MID - NAV_LINE / 2.0);
pub const NAV_SPINNER_AT_SMALL: (f32, f32) = (1.0, (NAV_ROW_H - NAV_LINE) / 2.0);
/// A full-size mark on a one-line (30px) Thread row, centred on it: at
/// the two-line row's x, the spinner's line and the dot centred.
pub const NAV_SPINNER_AT_LINE: (f32, f32) = (NAV_SPINNER_AT.0, NAV_SPINNER_AT_SMALL.1);
pub const NAV_DOT_AT_LINE: (f32, f32) = (NAV_DOT_AT.0, (NAV_ROW_H - NAV_DOT) / 2.0);
pub const NAV_DOT_AT: (f32, f32) = (4.0, NAV_LINE1_MID - NAV_DOT / 2.0);
pub const NAV_DOT_AT_SMALL: (f32, f32) = (3.0, (NAV_ROW_H - NAV_DOT_SMALL) / 2.0);
pub const NAV_ICON_AT: (f32, f32) = (0.0, NAV_LINE1_MID - NAV_MARK_ICON / 2.0);
/// An icon on a one-line row, a shown Group's member's included.
pub const NAV_ICON_AT_LINE: (f32, f32) = (0.0, (NAV_ROW_H - NAV_MARK_ICON) / 2.0);
pub const NAV_MARK_ICON: f32 = 14.0;
pub const NAV_MARK_ICON_SMALL: f32 = 13.0;
pub const NAV_SPINNER_FS: f32 = 12.0;
/// The parked mark's `terminal`, at half ink.
pub const NAV_PARKED_OPACITY: f32 = 0.5;
/// `#9a9a9a` — line 2 of the selected row, a step over `TEXT_DIM`.
pub const NAV_LINE2_SELECTED: u32 = 0x9a9a9a;
/// 18px — the needs-you pill (radius 9, padding 0 5, `ATTENTION` at 16%).
pub const NAV_PILL: f32 = 18.0;
pub const NAV_PILL_PAD_X: f32 = 5.0;
pub const NAV_PILL_FILL: u32 = 0xe6c47c29;
/// The blocked dot's halo: `ATTENTION` at 15%.
pub const NAV_HALO_FILL: u32 = 0xe6c47c26;
/// 12px — a Group row's chevron, at x 11 (`NAV_CHEVRON_X`).
pub const NAV_CHEVRON: f32 = 12.0;
pub const NAV_CHEVRON_X: f32 = 11.0;
/// 36px — the foot: `Ferrite 0.5` and the gear, 8px under the tree.
pub const NAV_FOOT_PAD_T: f32 = SPACE_2;
/// The most characters a folded Project's branch takes before it is cut.
pub const NAV_BRANCH_MAX_CH: usize = 24;
/// 4px — a drop target that takes no layout of its own ("insert above this
/// Group", "append after the last member"): an absolute hit band over the
/// edge of the row it borders.
pub const NAV_DROP_BAND: f32 = SPACE_1;
/// 52px — the column folded by cmd-B: the rail. One `NAV_RAIL_CELL` per
/// open Thread, holding its mark and, under it, its ⌘ digit in 11
/// `TEXT_DIM`.
pub const NAV_FOLDED_W: f32 = 52.0;
pub const NAV_RAIL_CELL: f32 = 32.0;
pub const NAV_RAIL_KEY_FS: f32 = 11.0;
/// The nav's scrollbar is the overlay every scroller has, laid in the
/// column's right padding (`NAV_COL_PAD`) rather than over the rows, so it
/// never covers a row's count or cuts its fill: an 8px hit strip with the
/// thumb `NAV_SCROLLBAR_INSET` from the column's edge. It shows while
/// scrolling and while the pointer is over the nav, then lingers and fades
/// as every thumb does (`MOTION_SCROLLBAR_LINGER_MS`).
pub const NAV_SCROLLBAR_GUTTER: f32 = NAV_COL_PAD;
/// 1px — off the space scale because the 6px hovered thumb must fit the
/// 8px padding with even air on both sides: 1 + 6 + 1.
pub const NAV_SCROLLBAR_INSET: f32 = 1.0;
const _: () = assert!(NAV_SCROLLBAR_GUTTER == 2. * NAV_SCROLLBAR_INSET + SCROLLBAR_THUMB_W_HOVER);
/// Narrow windows fold the nav (rule 2.7.7) — shown, never saved — once the
/// board beside the full column would be narrower than this, or a board
/// cell narrower than the L2 floor (`INSTRUMENTS_WIDTH`). It unfolds only
/// once the window clears the threshold by `NAV_AUTO_RAIL_HYSTERESIS`, so a
/// resize at the edge never flickers it. cmd-B still overrides it either
/// way.
pub const NAV_AUTO_RAIL_BOARD_W: f32 = 560.0;
pub const NAV_AUTO_RAIL_HYSTERESIS: f32 = 24.0;
/// 80ms — a nav row's hover wash reaching full: quicker than the 150ms
/// blend, because the pointer sweeps these rows constantly.
pub const MOTION_NAV_HOVER_MS: u64 = 80;
/// 100ms — the sidebar toggle's hover, ground and glyph.
pub const MOTION_NAV_DOOR_MS: u64 = 100;
/// 150ms — the column's content fading out as it folds and in as it
/// opens, on CSS `ease`, apart from the width's 200ms ride.
pub const MOTION_NAV_FADE_MS: u64 = 150;
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
//   dismissals are frequent, and the closed state says it all. The
//   sidebar's width rides cmd-B (`MOTION_RESIZE_MS`), interruptibly, and
//   the panel's rides ⌥⌘B the same way. A board slot is no float: the
//   reader slides in and, a little quicker, out (`MOTION_SLOT_OPEN_MS` /
//   `MOTION_SLOT_CLOSE_MS`), its Pane giving way and taking the room back,
//   so the board never jumps.
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
//   or breathes.
// - **Interruptible.** A state the operator can flip back (a hover, a
//   chevron) retargets from where it is, never restarts.
// - **No entrance on first paint.** Only a change the operator watched
//   happen animates; a restored layout or scroll-back arrives in place.
// - **Every animation has a static end state** that says the same thing
//   without motion. Reduced motion (the system flag, `cx.reduce_motion()`)
//   snaps a one-shot to its end and holds a loop at its start.
// - **A window with nothing animating schedules no frame.** Loops ride one
//   shared, throttled pulse clock (`MOTION_PULSE_TICK_MS`): each declares
//   when its picture next changes, and the clock wakes only the views due
//   then, on its grid; it parks when nothing is declared.

/// Zeron's signature entrance curve, CSS `cubic-bezier(0.16, 1, 0.3, 1)`.
pub const MOTION_EASE_OUT_EXPO: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
/// CSS `ease`: quick fades, menu and sheet entrances.
pub const MOTION_EASE: [f32; 4] = [0.25, 0.1, 0.25, 1.0];
/// CSS `transition-colors`' default curve: every hover blend.
pub const MOTION_EASE_STANDARD: [f32; 4] = [0.4, 0.0, 0.2, 1.0];
/// A drawer's ease-out (the iOS sheet's curve): it leaves at speed and
/// spends its second half settling the last few pixels, so a slot sliding
/// across half the board reads as one physical move rather than a jump.
pub const MOTION_EASE_SLIDE: [f32; 4] = [0.32, 0.72, 0.0, 1.0];
/// `row-in`: 180ms on `MOTION_EASE_OUT_EXPO`, opacity only, nothing moves:
/// a line arriving inside a card already open (the usage card's legend).
pub const MOTION_ROW_IN_MS: u64 = 180;
/// Zeron's `fade-in`: a transcript row appended live fades up over 500ms
/// while rising `MOTION_FADE_IN_RISE` into place.
pub const MOTION_FADE_IN_MS: u64 = 500;
pub const MOTION_FADE_IN_RISE: f32 = 4.0;
/// The sidebar's width between column and rail (cmd-B), and the panel's
/// between shown and hidden (⌥⌘B) — the board's width rides it every
/// frame.
pub const MOTION_RESIZE_MS: u64 = 200;
/// A board seam's hover line fading in under the pointer: 120ms.
pub const MOTION_SEAM_FADE_MS: u64 = 120;
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
/// A board slot opening beside its owner (the reader beside its Pane)
/// slides in over 240ms on `MOTION_EASE_SLIDE`, the owner narrowing with
/// it, and slides out over a shorter 200ms before it is removed.
pub const MOTION_SLOT_OPEN_MS: u64 = 240;
pub const MOTION_SLOT_CLOSE_MS: u64 = 200;
/// A disclosure chevron turning: 150ms.
pub const MOTION_CHEVRON_MS: u64 = 150;
/// The image viewer's zoom (a click between fit and actual size, ⌘= ⌘-
/// ⌘0): 240ms on `MOTION_EASE_OUT_EXPO`, so the picture answers the click
/// at once and settles softly. A pinch or a pan follows the hand instead.
pub const MOTION_IMAGE_ZOOM_MS: u64 = 240;
/// The hover blend: 150ms on `MOTION_EASE_STANDARD`.
pub const MOTION_HOVER_FADE_MS: u64 = 150;
/// A toast's stack timing: it settles in over 180ms and leaves over 100ms.
pub const MOTION_TOAST_IN_MS: u64 = 180;
pub const MOTION_TOAST_OUT_MS: u64 = 100;
/// 1.4s — how long a scrollbar thumb holds after the last scroll frame
/// (C26) before it fades out over `MOTION_HOVER_FADE_MS`. Idle, no thumb.
pub const MOTION_SCROLLBAR_LINGER_MS: u64 = 1_400;
/// The pulse clock's grid: one ~30fps tick shared by every loop in the
/// window. A loop is drawn on the ticks where its picture changes; one that
/// stops painting drops off after its last declared tick, and the clock
/// parks.
pub const MOTION_PULSE_TICK_MS: u64 = 33;
/// 80ms — one frame of the braille spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, ten frames, an
/// 800ms turn) that stands in a working Thread's dot.
pub const MOTION_BRAILLE_FRAME_MS: u64 = 80;
/// 120ms — one frame of the working line's spinner (`· ✢ ✳ ✶ ✻ ✽ ✻ ✶ ✳ ✢`,
/// ten frames, 1.2s), Claude Code's own cadence.
pub const MOTION_WORKING_FRAME_MS: u64 = 120;
/// 2.4s — one sweep of the shimmer's crest across the working caption,
/// left to right.
pub const MOTION_SHIMMER_MS: u64 = 2_400;
/// The crest's half-width, as a fraction of the caption's width (the
/// prototype's 35%–50%–65% stops over a 300% gradient).
pub const SHIMMER_HALF_WIDTH: f32 = 0.45;
/// 1.1s — the focused Composer's soft block caret: full for 45% of the
/// period, eased down to `CARET_BLINK_MIN` and held, eased back up.
pub const MOTION_CARET_BLINK_MS: u64 = 1_100;
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
                _ => TEXT_DIM,
            };
            assert_eq!(word_ink(word), expected, "{word:?}");
        }
        assert_eq!(word_ink("needs you"), ATTENTION);
        assert_eq!(word_ink("failed"), BLOCKED);
        assert_eq!(word_ink("failing"), BLOCKED);
        assert_eq!(word_ink("done"), TEXT_DIM);
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

    /// The grounds a readable row rests on (rule 5).
    const GROUNDS: &[(&str, u32)] = &[
        ("FRAME", FRAME),
        ("CARD", CARD),
        ("BAR", BAR),
        ("FIELD", FIELD),
        ("FIELD_COMPACT", FIELD_COMPACT),
        ("DECISION", DECISION),
        ("SUNKEN", SUNKEN),
    ];
    /// The raised grounds: a chip, a float, the selected row.
    const RAISED_GROUNDS: &[(&str, u32)] = &[
        ("TAB_ACTIVE", TAB_ACTIVE),
        ("RAISED", RAISED),
        ("FLOAT", FLOAT),
        ("SELECTED", SELECTED),
    ];

    #[test]
    fn ink_clears_its_floor_on_every_ground() {
        // Readable text: AA on every ground, raised ones included, down to
        // `TEXT_MUTED`.
        let readable = &[
            ("TEXT_STRONG", TEXT_STRONG),
            ("TEXT_PROSE", TEXT_PROSE),
            ("TEXT", TEXT),
            ("TEXT_BAR", TEXT_BAR),
            ("TEXT_MUTED", TEXT_MUTED),
        ];
        floor(readable, GROUNDS, 4.5);
        floor(readable, RAISED_GROUNDS, 4.5);
        // `TEXT_DIM` is the floor (SPEC §3.3): 4.5:1 on the frame, the card
        // and the grounds rows sit on, 4:1 on the raised ones.
        floor(&[("TEXT_DIM", TEXT_DIM)], GROUNDS, 4.5);
        floor(&[("TEXT_DIM", TEXT_DIM)], RAISED_GROUNDS, 4.0);
        // A float's rows: its dim words under the pointer, and lifted to
        // `TEXT_MUTED` on the cursor row.
        floor(
            &[("TEXT_DIM", TEXT_DIM)],
            &[("FLOAT_RAISED", FLOAT_RAISED)],
            4.0,
        );
        floor(
            &[("TEXT_MUTED", TEXT_MUTED)],
            &[("FLOAT_SELECTED", FLOAT_SELECTED)],
            4.5,
        );
        // Each step reads over the one under it.
        const { assert!(FLOAT_RAISED > FLOAT && FLOAT_SELECTED > FLOAT_RAISED) };
        assert!(contrast(TEXT_DIM, FRAME) >= 4.5 && contrast(TEXT_DIM, CARD) >= 4.5);
        assert!(contrast(TEXT_DIM, SELECTED) >= 4.0);
        // Comments are the dim ink, under its floor.
        assert_eq!(SYN_COMMENT, TEXT_DIM);
        // Structure only, never text: a visible glyph on its grounds.
        floor(
            &[("TEXT_FAINT", TEXT_FAINT)],
            &[
                ("CARD", CARD),
                ("FRAME", FRAME),
                ("FLOAT", FLOAT),
                ("BAR", BAR),
            ],
            2.0,
        );
        // State and syntax read as text on the grounds they are written on.
        floor(
            &[
                ("RUNNING", RUNNING),
                ("ATTENTION", ATTENTION),
                ("BLOCKED", BLOCKED),
                ("SYN_KEYWORD", SYN_KEYWORD),
                ("SYN_FUNCTION", SYN_FUNCTION),
                ("SYN_TYPE", SYN_TYPE),
                ("SYN_STRING", SYN_STRING),
                ("SYN_NUMBER", SYN_NUMBER),
                ("SYN_PUNCT", SYN_PUNCT),
                ("SYN_PLAIN", SYN_PLAIN),
                ("INLINE_CODE_INK", INLINE_CODE_INK),
                ("LINK_INK", LINK_INK),
                ("DIFF_ADDED_INK", DIFF_ADDED_INK),
                ("DIFF_REMOVED_INK", DIFF_REMOVED_INK),
            ],
            &[
                ("CARD", CARD),
                ("FRAME", FRAME),
                ("FLOAT", FLOAT),
                ("BAR", BAR),
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
                &[("a diff wash on CARD", over(wash, CARD))],
                4.5,
            );
        }
        // A primary button's label, at rest and under the pointer.
        floor(
            &[("ON_PRIMARY", ON_PRIMARY)],
            &[("PRIMARY", PRIMARY), ("TEXT", TEXT)],
            4.5,
        );
        // Control boundaries, keyboard focus and a provider's mark: 3:1.
        floor(
            &[
                ("CONTROL_FOCUS", CONTROL_FOCUS),
                ("INPUT_EDGE", INPUT_EDGE),
                ("PROVIDER_CLAUDE", PROVIDER_CLAUDE),
                ("PROVIDER_CODEX", PROVIDER_CODEX),
            ],
            &[
                ("CARD", CARD),
                ("FRAME", FRAME),
                ("FLOAT", FLOAT),
                ("FIELD", FIELD),
            ],
            3.0,
        );
        // Ink stays readable on the washes painted under it, and the find's
        // orange reads apart from the white selection.
        floor(
            &[("TEXT_STRONG", TEXT_STRONG)],
            &[("DROP_WASH on CARD", over(DROP_WASH, CARD))],
            4.5,
        );
        floor(
            &[("TEXT", TEXT)],
            &[(
                "TEXT_SELECTION_WASH on CARD",
                over(TEXT_SELECTION_WASH, CARD),
            )],
            4.5,
        );
        let (find, selected) = (over(FIND_WASH, CARD), over(TEXT_SELECTION_WASH, CARD));
        assert!(
            (find >> 16) & 0xff > (find & 0xff) + 8,
            "the find wash stays warm"
        );
        assert_ne!(find, selected);
    }

    /// Rule 5: the ink ladder, brightest first.
    #[test]
    fn the_ink_ladder_steps_down() {
        let ladder = [
            TEXT_STRONG,
            TEXT_PROSE,
            TEXT,
            TEXT_BAR,
            TEXT_MUTED,
            TEXT_DIM,
            TEXT_FAINT,
        ];
        assert_eq!(
            ladder,
            [0xf2f2f2, 0xe6e6e6, 0xd0d0d0, 0xc0c0c0, 0xa0a0a0, 0x8c8c8c, 0x5e5e5e]
        );
        for pair in ladder.windows(2) {
            assert!(luminance(pair[0]) > luminance(pair[1]), "{pair:06x?}");
        }
    }

    /// Rule 3: the surfaces, darkest first, every one opaque and neutral
    /// grey (no hue anywhere but state), and no accent.
    #[test]
    fn the_surface_ladder_is_ordered_and_grey() {
        let ladder = [
            ("NODIFF", NODIFF),
            ("SUNKEN", SUNKEN),
            ("CARD", CARD),
            ("BAR", BAR),
            ("DECISION", DECISION),
            ("FIELD_COMPACT", FIELD_COMPACT),
            ("FRAME", FRAME),
            ("TAB_ACTIVE", TAB_ACTIVE),
            ("RAISED", RAISED),
            ("SELECTED", SELECTED),
            ("SELECTION_HOVER", SELECTION_HOVER),
            ("SEND", SEND),
        ];
        for pair in ladder.windows(2) {
            assert!(
                luminance(pair[0].1) < luminance(pair[1].1),
                "{} must sit below {}",
                pair[0].0,
                pair[1].0
            );
        }
        for (name, face) in ladder {
            assert!(face <= 0xffffff, "{name} carries alpha");
            let (r, g, b) = (face >> 16, (face >> 8) & 0xff, face & 0xff);
            assert!(r == g && g == b, "{name} #{face:06x} is not a neutral grey");
        }
        assert_eq!(
            [FRAME, CARD, BAR, FIELD, RAISED, SELECTED],
            [0x222222, 0x181818, 0x1e1e1e, 0x222222, 0x2a2a2a, 0x2e2e2e]
        );
        assert_eq!((PRIMARY, ON_PRIMARY), (TEXT_STRONG, 0x161616));
        // The hairlines are white at their stated opacities.
        let alpha = |rgba: u32| (rgba & 0xff) as f32 / 255.;
        for (line, opacity) in [
            (HAIRLINE, 0.06),
            (CARD_RING, 0.07),
            (FIELD_RING, 0.08),
            (HAIRLINE_STRONG, 0.10),
            (ROW_RING, 0.06),
            (BAR_TOP, 0.05),
            (FOCUS_RING, 0.22),
            (DROP_EDGE, 0.22),
            (DROP_WASH, 0.06),
        ] {
            assert_eq!(line >> 8, 0xffffff, "{line:08x} is white");
            assert!((alpha(line) - opacity).abs() < 0.005, "{line:08x}");
        }
        assert_eq!(WAIT_RING >> 8, ATTENTION, "the wait ring is yellow");
        assert!((alpha(WAIT_RING) - 0.35).abs() < 0.005);
    }

    /// No accent: no ink the chrome wears is blue, and the retired names
    /// stay retired (ADR 0033).
    #[test]
    fn the_accent_is_retired() {
        let source = include_str!("theme.rs");
        let head = &source[..source.find("\n#[cfg(test)]\nmod tests").unwrap()];
        for retired in [
            "ACCENT",
            "ON_ACCENT",
            "ACCENT_HI",
            "PATH_INK",
            "MODE_INK",
            "FINISHED",
        ] {
            let decl = format!("pub const {retired}:");
            assert!(!head.contains(&decl), "{retired} is retired");
        }
        for (name, ink) in [
            ("CARET", CARET),
            ("CONTROL_FOCUS", CONTROL_FOCUS),
            ("LINK_INK", LINK_INK),
            ("INLINE_CODE_INK", INLINE_CODE_INK),
            ("MENTION_INK", MENTION_INK),
            ("PRIMARY", PRIMARY),
        ] {
            assert_eq!(ink, TEXT_STRONG, "{name} is the strongest ink");
        }
        assert_eq!(mode_ink("bypassPermissions"), ATTENTION);
        assert_eq!(mode_ink("danger-full-access"), ATTENTION);
        assert_eq!(mode_ink("acceptEdits"), TEXT_MUTED);
        assert_eq!(IDLE, TEXT_DIM, "done and idle wear no hue");
    }

    /// Rule 4: glass on macOS — the window paints nothing, the frame and
    /// the card are translucent at their stated opacities, overlays are
    /// white, a float stays opaque — and the opaque greys everywhere else.
    #[test]
    fn the_paints_are_glass_on_macos_and_grey_elsewhere() {
        let alpha = |rgba: u32| (rgba & 0xff) as f32 / 255.;
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(near(alpha(CARD_GLASS), 0.94));
        assert!(
            near(alpha(FRAME_GLASS), 0.88),
            "dark enough over a white window behind"
        );
        assert_eq!(alpha(FLOAT_GLASS), 1.0, "a float hides what it covers");
        for (overlay, opacity) in [
            (FIELD_GLASS, 0.05),
            (RAISED_GLASS, 0.06),
            (SELECTED_GLASS, 0.11),
            (SELECTION_HOVER_GLASS, 0.14),
            (LINE_GLASS, 0.08),
            (LINE2_GLASS, 0.14),
        ] {
            assert_eq!(overlay >> 8, 0xffffff, "{overlay:08x} is a white overlay");
            assert!(near(alpha(overlay), opacity), "{overlay:08x}");
        }
        // Glass overlays step up in the same order as the opaque ladder.
        const {
            assert!(FIELD_GLASS & 0xff < RAISED_GLASS & 0xff);
            assert!(RAISED_GLASS & 0xff < SELECTED_GLASS & 0xff);
            assert!(SELECTED_GLASS & 0xff < SELECTION_HOVER_GLASS & 0xff);
        }
        let paints = [
            (paint::CARD, CARD_GLASS, CARD),
            (paint::FRAME, FRAME_GLASS, FRAME),
            (paint::BAR, BAR_GLASS, BAR),
            (paint::FIELD, FIELD_GLASS, FIELD),
            (paint::RAISED, RAISED_GLASS, RAISED),
            (paint::SELECTED, SELECTED_GLASS, SELECTED),
            (
                paint::SELECTION_HOVER,
                SELECTION_HOVER_GLASS,
                SELECTION_HOVER,
            ),
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
        // The hairlines are the same white overlay on every platform.
        assert_eq!(paint::HAIRLINE.rgba(), HAIRLINE);
        assert_eq!(paint::HAIRLINE_STRONG.rgba(), HAIRLINE_STRONG);
        assert_eq!(paint::PRESS, paint::SELECTION_HOVER);
        // A float is opaque on every platform: glass picks its own grey.
        assert!(paint::FLOAT.is_opaque());
        assert_eq!(
            paint::FLOAT.rgba(),
            if GLASS { FLOAT_GLASS } else { solid(FLOAT) }
        );
        if GLASS {
            assert_eq!(paint::WINDOW.rgba(), TRANSPARENT, "the root paints nothing");
            assert_eq!(
                window_background(),
                gpui::WindowBackgroundAppearance::Blurred
            );
        } else {
            assert_eq!(paint::WINDOW.rgba(), solid(CARD));
            assert_eq!(
                window_background(),
                gpui::WindowBackgroundAppearance::Opaque
            );
        }
        // A paint converts the way `rgba()` does, never the way `rgb()` would.
        let fill: gpui::Fill = paint::CARD.into();
        assert_eq!(fill, gpui::Fill::from(gpui::rgba(paint::CARD.0)));
        let hsla: gpui::Hsla = paint::RAISED.into();
        assert_eq!(hsla, paint::RAISED.hsla());
    }

    /// Rule 2: radii by role (SPEC §3.6), and only floats cast.
    #[test]
    fn radii_follow_their_roles_and_only_floats_cast() {
        assert_eq!(
            [R_INLINE, R_SMALL, R_TAB, R_CONTROL, R_CHIP, R_BLOCK, R_PANE, R_PALETTE],
            [4.0, 5.0, 6.0, 7.0, 7.0, 8.0, 10.0, 12.0]
        );
        assert_eq!((R_TIGHT, R_MENU_ROW), (2.0, R_TAB));
        const { assert!(SHADOW_FLOAT_Y > 0.0 && SHADOW_FLOAT_BLUR > 0.0) };
    }

    /// Rule 1: every type role on a whole-pixel line, a 1.6× terminal row
    /// at every reading size, headings at the body size, a code cell of
    /// 0.6em.
    #[test]
    fn every_type_role_has_a_whole_pixel_line_box() {
        use ferrite_core::settings::ReadingSize;
        assert_eq!((FS_UI, LH_UI), (13.0, 20.0), "the grid is 13 on 20");
        for (size, line) in [(FS_PROSE, LH_PROSE), (FS_UI, LH_UI)] {
            assert_eq!(line, line.round());
            assert!(line >= size * 1.5, "{size}px on a {line}px line box");
        }
        for reading in ReadingSize::STEPS.map(ReadingSize::nearest) {
            let (size, line) = (answer_text_size(reading), answer_line_height(reading));
            assert_eq!(line, line.round());
            assert!(line >= size * 1.55, "{reading:?}: {size}/{line}");
            assert!(line <= size * 1.65, "{reading:?}: {size}/{line}");
        }
        // SPEC §3.5: Solo 13/21, a Group Pane 12.5/20.
        assert_eq!(answer_line_height(ReadingSize::nearest(13)), 21.0);
        let group = tx_size(ReadingSize::STANDARD, false);
        assert_eq!((group, prose_line_height(group)), (12.5, 20.0));
        assert_eq!(answer_text_size(ReadingSize::STANDARD), FS_PROSE);
        assert_eq!(answer_line_height(ReadingSize::STANDARD), LH_PROSE);
        for level in 1..=6 {
            assert_eq!(heading_scale(level), 1.0, "H{level} is the body size");
        }
        assert_eq!(prose_line_height(FS_PROSE), LH_PROSE);
        // The chrome's roles (SPEC §3.5).
        assert_eq!(
            [
                FS_TITLE,
                FS_LABEL,
                FS_META,
                FS_BADGE,
                FS_COMPOSER,
                FS_PALETTE
            ],
            [13.5, 12.5, 12.0, 11.0, 13.5, 14.5]
        );
        for line in [LH_META, LH_PANEL_DIFF] {
            assert_eq!(line, line.round());
        }
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(near(CODE_CELL, 7.8) && near(CH, CODE_CELL));
        assert!(near(GLYPH_GUTTER, 2.0 * CH));
        assert_eq!((ROW, HALF_ROW), (20.0, 10.0));
        assert_eq!((TITLEBAR_H, STATUS_BAR_H), (46.0, 30.0));
        assert_eq!((NAV_WIDTH, BOARD_PAD, GRID_GAP), (264.0, 8.0, 8.0));
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
            assert_eq!(tokens.button_primary.color, solid(PRIMARY));
            assert_eq!(tokens.button_primary_hover.color, solid(TEXT));
            assert_eq!(tokens.button_primary_foreground.color, solid(ON_PRIMARY));
            assert_eq!(tokens.primary.color, solid(PRIMARY));
            assert_eq!(tokens.accent.color, paint::SELECTED.hsla());
            assert_eq!(tokens.popover.color, paint::FLOAT.hsla());
            assert_eq!(tokens.muted.color, solid(RAISED));
            assert_eq!(tokens.tab_active.color, solid(SELECTED));
            assert_eq!(tokens.ring.color, solid(CONTROL_FOCUS));
            assert_eq!(tokens.input.color, solid(INPUT_EDGE));
            assert_eq!(tokens.border.color, paint::LINE2.hsla());
            assert_eq!(tokens.selection.color, alpha(TEXT_SELECTION_WASH));
            assert_eq!(tokens.caret.color, solid(TEXT_STRONG));
            assert_eq!(tokens.link.color, solid(TEXT_STRONG));
            assert_eq!(tokens.sidebar.color, solid(FRAME));
            assert_eq!(tokens.drop_target.color, alpha(DROP_WASH));
            // And the colours the non-token paths read.
            assert_eq!(theme.primary, solid(PRIMARY));
            assert_eq!(theme.info, solid(TEXT_STRONG), "no accent hue");
            assert_eq!(theme.muted, solid(RAISED));
            assert_eq!(theme.background, solid(CARD));
            assert!(!theme.shadow, "floats wear Ferrite's own shadow");
            assert_eq!(theme.radius, gpui::px(R_CONTROL));
            assert_eq!(theme.radius_lg, gpui::px(R_BLOCK));
            assert_eq!(theme.font_size, gpui::px(FS_UI));
            assert_eq!(theme.mono_font_size, gpui::px(FS_UI));
            assert_eq!(theme.font_family.as_ref(), "Geist");
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
    const GEIST: &[u8] = include_bytes!("../assets/fonts/Geist.ttf");

    /// Rule 1: two faces. The chrome's is Geist and the code's Geist Mono,
    /// and every bundled face is one of their five weights.
    #[test]
    fn faces_are_the_bundled_families() {
        assert_eq!((FONT_UI, FONT_CODE), ("Geist", "Geist Mono"));
        assert_eq!(family(GEIST), FONT_UI);
        assert_eq!(family(GEIST_MONO), FONT_CODE);
        for name in [FONT_UI, FONT_CODE] {
            let weights = crate::FONTS
                .iter()
                .filter(|face| family(face) == name)
                .count();
            assert_eq!(weights, 5, "{name}: regular, italic and three weights");
        }
        assert_eq!(crate::FONTS.len(), 10, "nothing else is bundled");
        assert_eq!(
            family(crate::FONTS[0]),
            FONT_CODE,
            "the baseline is measured on Geist Mono"
        );
    }

    /// Rule 10: what Geist Mono sets is in the face; what the platform's
    /// fallback sets is not.
    #[test]
    fn chrome_glyphs_are_in_the_face_and_fallback_glyphs_are_not() {
        assert!(covers(GEIST_MONO, 'a') && covers(GEIST_MONO, '$'));
        for glyph in CHROME_GLYPHS {
            assert!(covers(GEIST_MONO, *glyph), "{glyph} is not in Geist Mono");
        }
        for glyph in FALLBACK_GLYPHS {
            assert!(
                !covers(GEIST_MONO, *glyph),
                "{glyph} is in Geist Mono now (move it to CHROME_GLYPHS)"
            );
        }
        // The chrome's own glyphs in Geist (ADR 0033); `⌘` is the
        // platform's in both faces.
        for glyph in UI_GLYPHS {
            assert!(covers(GEIST, *glyph), "{glyph} is not in Geist");
        }
        for glyph in ['\u{2318}', '\u{2325}', '\u{2303}', '\u{276f}'] {
            assert!(
                !covers(GEIST, glyph),
                "{glyph} is in Geist now (move it to UI_GLYPHS)"
            );
            assert!(FALLBACK_GLYPHS.contains(&glyph));
        }
        // The spinner frames the design animates are the fallback's.
        for frame in crate::components::BRAILLE_FRAMES
            .iter()
            .chain(&crate::components::WORKING_FRAMES)
            .filter(|frame| **frame != '\u{b7}')
        {
            assert!(FALLBACK_GLYPHS.contains(frame), "{frame}");
        }
    }

    /// Every non-ASCII glyph render code puts in a literal is one rule 10
    /// knows: Geist Mono's own, or one the fallback face sets. Scans the
    /// render modules' non-test source, skipping comments.
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
            ("image_viewer.rs", include_str!("image_viewer.rs")),
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
                    if !covers(GEIST_MONO, c) && !FALLBACK_GLYPHS.contains(&c) {
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
