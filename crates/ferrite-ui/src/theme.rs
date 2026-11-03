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
//! bullets, `└` results, spinners, state inks — and keeps what only
//! a native app can do: vector marks, inline images, side-by-side diffs,
//! folds in place, pixel scrolling, floating pickers. Everything around the
//! transcript is quieter and modern: sans chrome, rounded cards on a
//! lighter frame, hairlines, and colour only for state. No chat bubbles, no
//! proportional transcript.
//!
//! The rules every render site follows:
//!
//! 1. **Two faces, one scale** (supersedes "one face, one grid"). What is
//!    read or touched — prose, nav, heads, tabs, buttons, menus, the
//!    palette, the panel, the bar, the Composer's input — is the system's
//!    UI font, SF Pro on macOS (`FONT_UI`, gpui's `.SystemUIFont`); code,
//!    diffs, commands and the terminal's numbers and glyphs (diffstats,
//!    spinners, `❯`) are Geist Mono (`FONT_CODE`, bundled). **One type
//!    scale, nothing else: 12 · 13 · 14 (base) · 16 · 18 · 24**
//!    (`TYPE_SCALE`). Chrome type is a set of roles on it (`FS_UI` 14/20
//!    default — rows and menu items — `FS_TITLE` 14, `FS_LABEL` 13,
//!    `FS_META` 13, …) with tabular figures; the transcript is the reading
//!    size (`answer_text_size`, 14/21 at Standard; cmd-= / cmd-- / cmd-0,
//!    the transcript only: the one place the scale yields to the
//!    operator's zoom) on its 1.5× line (`tx_size`), code a step down, and
//!    everything inside it scales with it, measured in cells (`tx_cell`).
//!    Chrome metrics are pixels on the space scale. **Three weights**:
//!    Regular (400, `W_BODY`) for what is read, Medium (500, `W_LABEL`) for
//!    titles, tabs, buttons and labels, Semibold (600, `W_STRONG`) for
//!    `**strong**`, table heads and headings. Nothing heavier. Never reach
//!    a weight by family name.
//! 2. **One plane, divided by hairlines** (supersedes "cards on a
//!    frame"). The window is one flat grey, `CARD` #1a1a1a: the nav, the
//!    head row, the main area, the right panel and the bottom bar are all
//!    it (`FRAME` is `CARD`). Regions part by `LINE` hairlines alone —
//!    one the window's width under the head row, one at the nav's edge, one
//!    at the panel's (`plane_seams` in the Cockpit), the bar's top —
//!    never by a second grey, an inset or a gap that reads. A Solo Pane is
//!    the plane itself, flush and unringed (`PaneEdge::Flush`); a Group's
//!    Panes keep a `CARD_RING` hairline round their cells, at `R_CARD`.
//!    **Three radii**: `R_CONTROL` 6 (controls, rows, chips), `R_CARD` 10
//!    (bubbles, cards, the Decision box) and `R_FLOAT` 12 (the Composer,
//!    menus, popovers, sheets); dots, pills and the send circle round.
//!    **Two line greys**: `LINE` #303030 for seams, dividers and table
//!    rules, `CARD_RING` #222222 for a card's ring. A raised surface edges
//!    itself a step over its own fill (`FIELD_RING` the Composer's,
//!    `FLOAT_EDGE` a float's and the rules inside it, `BLOCK_RING` a
//!    fence's), or a fill step separates: no keycap boxes, no boxed section
//!    labels. Only a floating surface casts, one shadow
//!    (`components::float_shadow`); a modal adds `VEIL`.
//! 3. **Neutral greys, steps over the plane.** Darkest first: `SUNKEN` (a
//!    command well) < `CARD` = `FRAME` (the plane) <
//!    `FIELD_COMPACT` < `RAISED` #242424 (an active tab or chip, secondary
//!    buttons) < `BLOCK` #262626 (diff and fence blocks; `PROMPT_BUBBLE`) <
//!    `SELECTED` #2a2a2a (**the one selected fill**: the selected row, tab,
//!    option) < `SELECTION_HOVER` (every press) < `SEND` < `FIELD` #303030
//!    (the Composer, the prompt band, a search field). `FLOAT` #303030
//!    grounds menus and popovers, its cursor row `FLOAT_SEL` #3e3e3e.
//!    `PRIMARY` (the strongest ink, with `ON_PRIMARY`) is the primary
//!    button. Hover is not a grey but a white overlay (`HOVER_WASH`,
//!    `paint::HOVER`): one equal small step (#1a1a1a → #1e1e1e) on
//!    whatever ground the control rests on, under the selected fill.
//! 4. **Opaque, flat, terminal in the plane.** The window is opaque on
//!    every platform (`WindowBackgroundAppearance::Opaque`): nothing of the
//!    desktop reads through, so the hierarchy is carried by the hairlines
//!    of rule 2 and the small steps of rule 3. The root paints the plane
//!    (`paint::WINDOW`); each region paints its own fill once, from
//!    `paint::*`, the same grey. Inside a region, fields, raised and
//!    selected fills are the next opaque steps. gpui blurs no element, so
//!    no surface is frosted: a float paints opaque (`paint::FLOAT`), and a
//!    surface that must occlude what scrolls under it (the sticky prompt, a
//!    Decision docked over the tail) paints a solid fill. Fills and lines
//!    go through `theme::paint` (typed `Paint`, so `rgb()` cannot swallow
//!    an `0xRRGGBBAA` by mistake); the bare consts are the `0xRRGGBB`
//!    values, for contrast math. **Never stack two region fills**: what
//!    sits inside a region paints its own step or nothing.
//! 5. **Two text tiers** (Claude's): every word is content or meta.
//!    `INK_STRONG` #efefef is content (what is read — prose, titles, row
//!    names, menu items, buttons, chips, the prompt, `**strong**`, `❯`, the
//!    caret, focus), and an icon wears its label's ink. `INK_SUBTLE` #9c9c9c
//!    is meta (descriptions, times, section headings, tool rows and
//!    arguments, hints; **the floor for readable text**: 4.5:1 on the plane
//!    and a float). There is no tier between them. A placeholder is
//!    `PLACEHOLDER_INK` #878787, under the floor: it is not read.
//!    `TEXT_FAINT` is structure only (`└`, rails, diff line numbers):
//!    **`TEXT_FAINT` is never text.** A row spends at
//!    most two text inks, one glyph ink and one colour (a diffstat's `+`/`−`
//!    pair may join a state's colour, ADR 0033).
//! 6. **No accent; colour is state** (supersedes "real terminal colour").
//!    Steel blue is retired: focus, selection, `❯` and the caret use
//!    `INK_STRONG`, and selection is a lighter fill, never a coloured
//!    outline. A link is the one ink that is not state: the Claude app's
//!    link blue (`LINK_INK`), underlined in prose — the owner's spec; and a
//!    menu's standing `✓` wears Claude's check blue (`MENU_CHECK_INK`). Hue
//!    is for state only: `RUNNING` green (working, `ok`, diff `+`),
//!    `ATTENTION` yellow (needs you), `MODE_ATTENTION_INK` amber (the
//!    permission mode that asks before every change, `mode_is_manual`),
//!    `BLOCKED` red (failed, diff
//!    `−`). Done is a shape (`check-circle`) in `INK_SUBTLE`, not a hue. A
//!    provider's brand colour (`PROVIDER_CLAUDE`, `PROVIDER_CODEX`) is only
//!    on its logomark and its working glyphs. Syntax colour (`SYN_*`) is for
//!    code in diffs and fences only; inline code and paths are `INK_STRONG`
//!    (a path in a tool's arguments `INK_SUBTLE`). A diff row washes red or
//!    green (`DIFF_*_WASH`,
//!    the changed words `DIFF_*_WORD`) under its code's own colours. The
//!    find wash (`FIND_WASH`) stays orange. **Colour sits on the word, the
//!    dot or the ring** — a blocked Pane's ring is `WAIT_RING`, a focused
//!    one's `FOCUS_RING` (white at 22%) — never on a ground.
//! 7. **One place per fact** (supersedes "clean": SPEC §5 places them). The
//!    bar is views and vitals; the Composer holds the mode, model and
//!    effort; the right panel holds changes, the pull request and context;
//!    a Group Pane's head speaks its state word. Nothing shows twice on one
//!    surface.
//! 8. **Little moves at once.** What moves: the working mark
//!    (`components::working_mark`, four white orbs swapping corners in
//!    pairs every `MOTION_WORKING_MARK_MS`), the app's one running glyph, its
//!    orbs white (red on a Thread whose run is failing) — the working
//!    line's mark, in place of the mark of every working Thread, and on a
//!    plan's current step and a running check; the working
//!    caption's shimmer (`components::shimmer`), on the focused Pane only;
//!    the caret blinking in the focused Composer (`components::caret_blink`);
//!    a button's hover fill blending in and its press squishing it. No dot pulses: a status dot never breathes, and
//!    unread is ink (`INK_STRONG` title), not motion. The motion section below holds
//!    the rest of the catalog, and reduced motion holds every loop on a
//!    static end state that says the same thing.
//! 9. **Pixel line heights and a space scale.** Every text role is a (size,
//!    line height) pair, and fixed row heights are `const` expressions of
//!    those pairs, never hand-summed literals. Space: 2 · 4 · 6 · 8 · 12 ·
//!    16 · 24 · 32 (`SPACE_*`, named in gpui's 4px units); inside the
//!    transcript, its line and cells. A metric off the scale says why in its
//!    doc.
//! 10. **Glyph coverage.** Every glyph is text, set in its face where the
//!     face has it and in the face the platform falls back to where it does
//!     not — on macOS Menlo for `❯ ◆ ✓ ✗ ▾ ▸ ⌘ ⌥ ⌃` and the stars
//!     (`✻`), STIX Two Math for `⏵ ⏸` (`components::glyph`) — on the
//!     line's baseline.
//!     `CHROME_GLYPHS` lists the non-ASCII glyphs Geist Mono sets and
//!     `FALLBACK_GLYPHS` the ones it lacks; a test checks each against the
//!     face (the system UI font covers the chrome's own, `⇧ ↑ ↓ · ⌘`).
//!     Icons are Ferrite's own line family (`icons.rs`), every stroke 1px
//!     at any drawn size (`ICON_STROKE`), drawn at 14–16.
//! 11. **Words.** Ferrite's own copy speaks one shared word list
//!     (`theme::words`, beside the state inks and tested against the
//!     notifications, the Decision card and the transcript): `needs you`,
//!     `failing N`, `failed`, `interrupted`, `working`, `done`. Titles,
//!     buttons, menu items, labels, chip labels and empty states are
//!     sentence case (`New thread`, `Review PR`); state and value tokens are
//!     lowercase, even leading a row; Title Case lives only in the macOS menu
//!     bar. `·` is the only separator inside a line — no colon labels, no
//!     em dash, no final period on one-line copy — and no surface prints
//!     `now`. A shortcut is plain `INK_SUBTLE` text, only where the operator
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

// ------------------------------------------------------------------ paint

/// An opaque `0xRRGGBB` as a `0xRRGGBBAA` paint.
pub const fn solid(rgb: u32) -> u32 {
    (rgb << 8) | 0xff
}

/// A fill or a line (rule 4), `0xRRGGBBAA`: an opaque grey or a white
/// overlay (a hairline). It converts into `Hsla`, `Rgba` and `Fill`, so `.bg(paint::CARD)` and
/// `.border_color(paint::LINE)` just work, and `rgb()` refuses it: the one
/// mistake a bare `u32` would let through silently (`rgb(0x1a1a1bf0)` drops
/// the red byte and paints blue).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Paint(pub u32);

impl Paint {
    /// An opaque `0xRRGGBB` grey.
    pub const fn solid(rgb: u32) -> Paint {
        Paint(solid(rgb))
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
// The opaque greys (`docs/design/ui-refresh/SPEC.md` §3.1), darkest first
// (rule 3). Paint a region with `paint::*`; the bare consts are the
// `0xRRGGBB` values, for contrast math.

/// `#1a1a1a` — **the one plane**: the main surface, a Pane's body, the
/// transcript's ground, and the chrome around it (`FRAME`) alike. Regions
/// are divided by `LINE` hairlines, never by a second grey.
pub const CARD: u32 = 0x1a1a1a;
/// The frame — the nav, the heads, the right panel, a Group's gutter, the
/// bottom bar: the plane itself (rule 2).
pub const FRAME: u32 = CARD;
/// `#272727` — the sidebar (the nav column): lighter than the plane, as
/// Claude's sidebar sits over its transcript (sampled from claude/*.png:
/// the transcript `#191919`, the sidebar `#292428` under the window's
/// vibrancy, ~14 levels over it; this is that grey without the wallpaper's
/// tint), so the column reads as its own region and not by its hairline
/// alone. The owner asked for it darker than Claude's: `#212121`, 8 levels
/// over the transcript. Its rows step up from it: `SIDEBAR_HOVER` (+8) under
/// the pointer, `SIDEBAR_SELECTED` (+32, Claude's step) selected, so a
/// hovered row never reads as the selected one.
pub const SIDEBAR: u32 = 0x212121;
pub const SIDEBAR_HOVER: u32 = 0x292929;
pub const SIDEBAR_SELECTED: u32 = 0x414141;
/// The standing choice's `✓` in a menu: Claude's check blue
/// (claude/model-menu.png x1266–1280, y744–755) — the owner's call, the one
/// hue in a menu that is not state (rule 6).
pub const MENU_CHECK_INK: u32 = 0x237ddb;
/// `#171717` — sunk below the plane: a Decision's command well, the
/// palette scrim's base.
pub const SUNKEN: u32 = 0x171717;
/// `#1f1f1f` — the compact Composer in a Group Pane.
pub const FIELD_COMPACT: u32 = 0x1f1f1f;
/// `#242424` — raised a step: an active tab or chip, secondary buttons,
/// the panel's file block.
pub const RAISED: u32 = 0x242424;
/// `#3b3b3b` — a secondary button on the plane that must read as one at a
/// glance (an approval's `Deny`, Claude's: +34 over the plane), and its
/// hover a step up.
pub const SECONDARY: u32 = 0x3b3b3b;
pub const SECONDARY_HOVER: u32 = 0x454545;
/// `#262626` — the prompt's bubble, matching Claude (typecolour.md).
pub const PROMPT_BUBBLE: u32 = 0x262626;
/// `#2a2a2a` — the inline-code chip, matching Claude (typecolour.md).
pub const INLINE_CODE_CHIP: u32 = 0x2a2a2a;
/// `#303030` — inline code's chip in a sent prompt, a step over its
/// `PROMPT_BUBBLE` (sampled from claude/running.png's user bubble).
pub const PROMPT_CODE_CHIP: u32 = 0x303030;
/// `#262626` — a code fence's block, under a `BLOCK_RING`: a fence
/// reads as a block at a glance.
pub const BLOCK: u32 = 0x262626;
/// `#2a2a2a` — **the one selected fill**: the selected nav row, the active
/// view and panel tab, a Decision option. (A float's is `FLOAT_SEL`.)
pub const SELECTED: u32 = 0x2a2a2a;
/// `#2d2d2d` — a selected row under the pointer, and every press: the
/// hover wash's step over the selected fill.
pub const SELECTION_HOVER: u32 = 0x2d2d2d;
/// `#303030` — a field on the plane: the Solo Composer's box, the prompt
/// band, a search field, under its `FIELD_RING`.
pub const FIELD: u32 = 0x303030;
/// `#404040` — the send button's circle (and a comment's avatar): a step
/// over the Composer it sits in.
pub const SEND: u32 = 0x404040;
/// `#303030` — the floating ground: menus, pickers, the palette, popovers,
/// tooltips, toasts, sheets, under its `FLOAT_EDGE` and its one shadow.
pub const FLOAT: u32 = 0x303030;
/// `#343434` · `#3e3e3e` — a float's hover and cursor fills, opaque steps
/// over its own ground. The cursor is the clear step; the hover is the
/// hover wash's, and the cursor row lifts its meta words to the content
/// ink (`components::row_inks`), as the nav's selected row lifts its line 2:
/// the meta ink falls under its floor on these steps.
pub const FLOAT_HOVER: u32 = 0x343434;
pub const FLOAT_SEL: u32 = 0x3e3e3e;
/// `#efefef` / `#161616` — the primary button (`primary_button`, dialogs
/// and settings) and its ink. No hue: the primary is the strongest ink.
pub const PRIMARY: u32 = INK_STRONG;
pub const ON_PRIMARY: u32 = 0x161616;
/// `#cfcfcf` — the primary button under the pointer and pressed: a fill
/// stepping down from `PRIMARY`, never an ink.
pub const PRIMARY_HOVER: u32 = 0xcfcfcf;

// ------------------------------------------------------------------ lines
//
// **Two line greys** on the plane (rule 2), and an edge per raised surface,
// each a step over its own fill. `0xRRGGBB`, painted through `paint::*`.
// The state rings and washes below are overlays.

/// 1px — every hairline's width (rule 2).
pub const HAIRLINE_W: f32 = 1.0;
/// `#303030` — **the line**: every region seam (the head row's foot, the
/// nav's and the panel's edges, the bar's top), every divider in a panel,
/// a table's rules, a code block's rail, an image's frame.
pub const LINE: u32 = 0x303030;
/// `#222222` — **the one transcript ring**, Claude's: round a card (a
/// Group's Pane, a tool or task card, a preview), round a hairline well (a
/// command, a diff), and every divider inside one — never brighter than the
/// card's edge. A ringed card has no fill.
pub const CARD_RING: u32 = 0x222222;
/// `#404040` — the 1px ring round the Composer and a form field.
pub const FIELD_RING: u32 = 0x404040;
/// `#383838` — a float's edge, and every rule inside a float (under its
/// head, over its footer): `LINE` would vanish on the float's ground.
pub const FLOAT_EDGE: u32 = 0x383838;
/// `#333333` — a code fence's and a command block's ring.
pub const BLOCK_RING: u32 = 0x333333;
/// 3.5% — the hover face of a control or row with no ground of its own
/// (rule 3): over its parent's ground it lands the same step everywhere,
/// `#1a1a1a` → `#1e1e1e` on the plane, where one opaque grey would step
/// twice as far on one ground as on another and vanish on a float.
pub const HOVER_WASH: u32 = 0xffffff04;
/// A chip whose menu is open: pressed, a 12% white wash (rgba) over
/// whatever ground it stands on — Claude's #363636 under its model and
/// permission chips on the card's #1a1a1a (claude/model-menu.png x1172,
/// y1018–1041), and its #4a4a4a under the box's `+` on the field's #303030
/// (claude/plus-menu.png). One opaque grey vanished on the field.
pub const CHIP_OPEN: u32 = 0xffffff1f;
/// 22% — the focused Pane on a Group (more than one Pane shows), a drop
/// target's edge.
pub const FOCUS_RING: u32 = 0xffffff38;
/// `ATTENTION` at 35% — the ring of a Pane whose Thread is blocked,
/// focused or not.
pub const WAIT_RING: u32 = 0xe6c47c59;
/// The compact Composer's ring on the focused Pane, 12% white (at rest it
/// is the `LINE`).
pub const COMPACT_RING_FOCUSED: u32 = 0xffffff1f;
/// The failure strip's fill: red at 7% over the plane (`#292221`).
pub const FAILURE_WASH: u32 = 0xef8a8012;
/// Fully transparent — an edge that is always in layout and only changes
/// colour, so nothing reflows when a Decision or a blocker arrives.
pub const TRANSPARENT: u32 = 0x00000000;

/// **The paints** (rule 4): what a region lays down. Every fill and line is
/// painted from here.
pub mod paint {
    use super::Paint;

    /// The window's own root, under every region: the card's grey.
    pub const WINDOW: Paint = CARD;
    pub const CARD: Paint = Paint::solid(super::CARD);
    pub const FRAME: Paint = Paint::solid(super::FRAME);
    /// The right panel: the frame's grey.
    pub const PANEL: Paint = FRAME;
    /// The nav column and its rows (`super::SIDEBAR`).
    pub const SIDEBAR: Paint = Paint::solid(super::SIDEBAR);
    pub const SIDEBAR_HOVER: Paint = Paint::solid(super::SIDEBAR_HOVER);
    pub const SIDEBAR_SELECTED: Paint = Paint::solid(super::SIDEBAR_SELECTED);
    pub const CHIP_OPEN: Paint = Paint(super::CHIP_OPEN);
    pub const ENV_STRIP: Paint = Paint::solid(super::ENV_STRIP);
    /// A diff's or a code fence's box, on a reading surface (the card, the
    /// panel).
    pub const BLOCK: Paint = Paint::solid(super::BLOCK);
    pub const FIELD: Paint = Paint::solid(super::FIELD);
    pub const RAISED: Paint = Paint::solid(super::RAISED);
    pub const PROMPT_BUBBLE: Paint = Paint::solid(super::PROMPT_BUBBLE);
    pub const INLINE_CODE_CHIP: Paint = Paint::solid(super::INLINE_CODE_CHIP);
    pub const PROMPT_CODE_CHIP: Paint = Paint::solid(super::PROMPT_CODE_CHIP);
    pub const SELECTED: Paint = Paint::solid(super::SELECTED);
    pub const SUNKEN: Paint = Paint::solid(super::SUNKEN);
    pub const SECONDARY: Paint = Paint::solid(super::SECONDARY);
    pub const SECONDARY_HOVER: Paint = Paint::solid(super::SECONDARY_HOVER);
    pub const FIELD_COMPACT: Paint = Paint::solid(super::FIELD_COMPACT);
    pub const SELECTION_HOVER: Paint = Paint::solid(super::SELECTION_HOVER);
    /// Every press: the selected row's hover face.
    pub const PRESS: Paint = SELECTION_HOVER;
    /// The hover face of a groundless control or row (`HOVER_WASH`).
    pub const HOVER: Paint = Paint(super::HOVER_WASH);
    /// The send circle.
    pub const SEND: Paint = Paint::solid(super::SEND);
    /// A float's ground, its edge (and the rules inside it), a row under
    /// the pointer and the cursor row (FL-14).
    pub const FLOAT: Paint = Paint::solid(super::FLOAT);
    pub const FLOAT_EDGE: Paint = Paint::solid(super::FLOAT_EDGE);
    pub const FLOAT_HOVER: Paint = Paint::solid(super::FLOAT_HOVER);
    pub const FLOAT_SEL: Paint = Paint::solid(super::FLOAT_SEL);
    /// The lines (rule 2): the seams and rules, a card's ring, the
    /// Composer's ring, a fence's ring.
    pub const LINE: Paint = Paint::solid(super::LINE);
    pub const CARD_RING: Paint = Paint::solid(super::CARD_RING);
    /// A selection's Reply pill: its ground, its edge, and its ground
    /// under the pointer (the edge's grey, Claude's white at 10%).
    pub const SELECTION_REPLY_FILL: Paint = Paint::solid(super::SELECTION_REPLY_FILL);
    pub const SELECTION_REPLY_EDGE: Paint = Paint::solid(super::SELECTION_REPLY_EDGE);
    pub const SELECTION_REPLY_HOVER: Paint = Paint::solid(super::SELECTION_REPLY_EDGE);
    pub const FIELD_RING: Paint = Paint::solid(super::FIELD_RING);
    pub const BLOCK_RING: Paint = Paint::solid(super::BLOCK_RING);

    /// The prompt band: a field that occludes what scrolls under it.
    pub const PROMPT_BAND: Paint = FIELD;
}

// ------------------------------------------------------------------ edges

/// `#7c7c7c` — a resting checkbox, radio or switch boundary: solid and at
/// least 3:1 on the card, the frame, a float and a field, so an unchecked
/// control never vanishes.
pub const INPUT_EDGE: u32 = 0x7c7c7c;
/// `#5e5e5e` / `#6d6d6d` — the scrollbar thumb, at rest and under the
/// pointer (the Claude app's thumb and its picker's).
pub const SCROLLBAR: u32 = 0x5e5e5e;
pub const SCROLLBAR_HOVER: u32 = 0x6d6d6d;
/// Black at 35% — the veil behind a modal sheet.
pub const VEIL: u32 = 0x00000059;
/// Black at 85% — the lightbox's dim, under which the window all but goes.
pub const LIGHTBOX_VEIL: u32 = 0x000000d9;

// -------------------------------------------------------------------- ink

/// **The two text tiers** (rule 5, Claude's): every word is one of these,
/// and an icon wears its label's.
/// `#efefef` — content: what is read (prose), titles, row names, menu
/// items, buttons, chips, selected rows, the prompt, `**strong**`, the
/// caret, focus.
pub const INK_STRONG: u32 = 0xefefef;
/// `#9c9c9c` — meta: descriptions, metadata, times, section headings, tool
/// rows and arguments, hints. **The floor for readable text**: 4.5:1 on the
/// plane and on a float.
pub const INK_SUBTLE: u32 = 0x9c9c9c;
/// `#878787` — a field's placeholder: under the subtle tier, because it is
/// not read but replaced.
pub const PLACEHOLDER_INK: u32 = 0x878787;
/// `#5e5e5e` — structure, never words: `└`, diff line numbers, rails,
/// disclosure glyphs, the parked ring. 2:1 on the float's lighter ground,
/// as on the plane.
pub const TEXT_FAINT: u32 = 0x5e5e5e;

// --------------------------------------------------------------- palette
//
// The named hues: state, syntax, the context legend. Semantic tokens below
// point at them; render code names the semantic token.

pub const RED: u32 = 0xf3767b;
pub const GREEN: u32 = 0x93cf8c;
pub const YELLOW: u32 = 0xe6c47c;
pub const BLUE: u32 = 0x8cb9f4;
pub const MAGENTA: u32 = 0xc59df0;
pub const CYAN: u32 = 0x78ccd0;
pub const ORANGE: u32 = 0xeda879;

// ----------------------------------------------------- focus and selection

/// Keyboard focus on a control (`components::control_focus`, the kit's
/// `ring`): the strongest ink, never a hue (rule 2).
pub const CONTROL_FOCUS: u32 = INK_STRONG;
/// White at 16% — native text selection, painted over glyphs.
pub const TEXT_SELECTION_WASH: u32 = 0xffffff29;
/// `#eda87933` (20%) — every occurrence a find (⌘F) marks, under its
/// glyphs: orange, a hue no state wears, kept apart from the white
/// selection and from yellow, which means "needs you".
pub const FIND_WASH: u32 = 0xeda87933;
/// `#eda87980` (50%) — the occurrence the find is on.
pub const FIND_ACTIVE_WASH: u32 = 0xeda87980;
/// The caret in the focused Composer.
pub const CARET: u32 = INK_STRONG;
/// A board seam under the pointer or held: `INK_STRONG` at 40%.
pub const SEAM_HOVER: u32 = 0xefefef66;

// -------------------------------------------------------- state + signals

/// Green — live work: the running dot and working mark, `ok`, a passing
/// check, diff `+`.
pub const RUNNING: u32 = GREEN;
/// Yellow — needs you: the dot, the `◆`, the kind word.
pub const ATTENTION: u32 = YELLOW;
/// `#e49b31` amber — the permission mode that stops for the operator before
/// every change, its word in the Composer's footer alone (the Claude app's
/// orange `Manual`; its mode-menu row is neutral): yellow would read as
/// "pending".
pub const MODE_ATTENTION_INK: u32 = 0xe49b31;
/// Red — failed: the dot, the word `failed`, diff `−`.
pub const BLOCKED: u32 = RED;
/// Red as a one-line ground (a refused drop, a destructive control under
/// the pointer); never a multi-line wash (`DIFF_REMOVED_WASH`).
pub const BLOCKED_WASH: u32 = 0xf3767b1f;
/// The idle/parked dot: the dim ink in a dot role. (Done is a shape, the
/// check-circle, in this ink too: no hue, ADR 0033.)
pub const IDLE: u32 = INK_SUBTLE;

/// Whether a permission mode is Claude's `Manual` (its `default`: ask
/// before every change): the one mode whose footer word draws in
/// `MODE_ATTENTION_INK`, as the Claude app's does (its menu row stays
/// neutral). Every other mode,
/// the ones that ask less (accept edits, bypass) among them, wears the
/// plain meta ink.
pub fn mode_is_manual(mode: &str) -> bool {
    matches!(mode, "" | "default")
}

/// A permission mode's one-line description in the mode menu, by its id
/// (Claude's and Codex's), as Claude's own menu words them; `None` for a
/// mode Ferrite does not know.
pub fn mode_note(mode: &str) -> Option<&'static str> {
    Some(match mode {
        "default" => "Always ask before making changes",
        "acceptEdits" => "Automatically accept all file edits",
        "plan" => "Create a plan before making changes",
        "bypassPermissions" => "Accepts all permissions",
        "dontAsk" => "Deny anything not already allowed",
        "auto" => "Claude handles permission decisions",
        "untrusted" => "Ask before running untrusted commands",
        "on-request" => "Ask when a command needs approval",
        "never" => "Never ask; failures go back to the model",
        _ => return None,
    })
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
    /// The pill over a waiting Decision card (Claude's words; a pill's
    /// label, so sentence case).
    pub const NEEDS_INPUT: &str = "Needs your input";
    /// `NEEDS_YOU` counted past one (`2 need you`).
    pub const NEED_YOU: &str = "need you";

    /// A count of waiting agents: `1 needs you`, `2 need you`. The nav's
    /// tally and the bottom bar both say it so.
    pub fn needs_you(count: usize) -> String {
        let word = if count == 1 { NEEDS_YOU } else { NEED_YOU };
        format!("{count} {word}")
    }
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
/// `INK_SUBTLE` — colour always means something needs you.
pub fn word_ink(word: &str) -> u32 {
    match word {
        words::NEEDS_YOU => ATTENTION,
        words::FAILED | words::FAILING => BLOCKED,
        _ => INK_SUBTLE,
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
/// green. Only the logomark and the working mark wear it: never a label, a
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
/// Comments are read, not decoration: `INK_SUBTLE`, set italic.
pub const SYN_COMMENT: u32 = INK_SUBTLE;
/// Punctuation: the plain ink (the prototype leaves `::`, `(`, `;` unclassed).
pub const SYN_PUNCT: u32 = SYN_PLAIN;
/// Everything the highlighter leaves unclassed: the content ink, as
/// Claude's and Codex's plain code.
pub const SYN_PLAIN: u32 = INK_STRONG;
/// A link or a path in prose, a file chip's name: the Claude app's link
/// blue, a link in prose underlined at rest (the owner's spec, superseding
/// ADR 0033's white links; a link is the one ink here that is not state).
/// A path in a tool call's arguments is `INK_SUBTLE` with the rest of them.
pub const LINK_INK: u32 = 0x7aa8ee;
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
// **Two faces** (rule 1). Everything read or touched is the system UI
// font (`FONT_UI`, SF Pro) at `FS_UI` 14 on `LH_UI` 20 by default, with
// the roles below (`FS_TITLE`, `FS_LABEL`, `FS_META`, …), every size on
// `TYPE_SCALE`. Code is Geist Mono (`FONT_CODE`). The transcript is the
// reading size on its own 1.5× line (`answer_text_size`,
// `answer_line_height`; 14/21 at Standard: `tx_size`) and every
// transcript measure scales from it.
//
// **Weights:** `W_BODY` (400) for everything that is read; `W_LABEL` (500)
// for tabs, buttons, Pane titles, labels, a view title, a tool's name, a
// badge count; `W_STRONG` (600) for `**strong**`, a table's head and a
// heading — the references' semibold, where 500 barely reads over 400.
// Nothing is heavier, and no render site names a `FontWeight` itself.
// Weight never signals state on a row that can truncate.
//
// **Figures:** the chrome sets tabular figures (`components::tabular` on
// the window root), so a ticking count or a column of numbers never
// shifts. Geist Mono's figures are tabular already, and it ships no
// ligatures (`theme::tests`).

/// 14px — the chrome's size: every chrome row and menu item, on `LH_UI`.
pub const FS_UI: f32 = 14.0;
/// 20px — the grid's row.
pub const LH_UI: f32 = 20.0;
/// 13px — the sidebar's rows and headings, a step under the chrome, as
/// Claude's sidebar sets them (measured: its row text spans 13px advances).
pub const FS_NAV: f32 = 13.0;
/// 14px — the Standard reading size (`ReadingSize::STANDARD`): agent prose
/// and every Solo transcript row at the default zoom, on `LH_PROSE`.
pub const FS_PROSE: f32 = 14.0;
/// 21px — the Standard reading size's line: 1.5×, whole
/// (`prose_line_height`).
pub const LH_PROSE: f32 = 21.0;
/// 14 on 20 — a view title (the Solo and Group heads), at `W_LABEL`.
pub const FS_TITLE: f32 = 14.0;
/// 13 — tabs, buttons, Pane titles, panel tabs, at `W_LABEL` on `LH_UI`.
pub const FS_LABEL: f32 = 13.0;
/// 13 on 16 (or `LH_UI`) — metadata, section headings, menu descriptions,
/// the bar, a row's second line.
pub const FS_META: f32 = 13.0;
pub const LH_META: f32 = 16.0;
/// 12 — a badge's count, at `W_LABEL`.
pub const FS_BADGE: f32 = 12.0;
/// 14 on 20 — the Solo Composer's input (a Group's is `FS_UI`).
pub const FS_COMPOSER: f32 = 14.0;
/// 12 on 18 — the panel's diff, in Geist Mono.
pub const FS_PANEL_DIFF: f32 = 12.0;
pub const LH_PANEL_DIFF: f32 = 18.0;
/// 12 — a diffstat, a spinner or a `#N` set in Geist Mono inside chrome.
pub const FS_CHROME_CODE: f32 = 12.0;
/// 1px — every line icon's stroke as drawn, whatever its viewBox and the
/// size it is drawn at (rule 10; the owner's call over Claude's 1.25). The
/// baked `stroke-width` would scale with the drawn size (Lucide's 2 on 24 is
/// 1.33 at 16, 1.17 at 14); `icons::icon` restrokes each icon for its size.
pub const ICON_STROKE: f32 = 1.0;
/// **The type scale** (rule 9): every chrome size is one of these.
pub const TYPE_SCALE: [f32; 6] = [12.0, 13.0, 14.0, 16.0, 18.0, 24.0];

/// Weights (see the type notes above): Regular (400), Medium (500) and
/// Semibold (600), nothing else. `W_STRONG` is the emphasis of
/// `**strong**`, a table's head and a heading.
pub const W_BODY: FontWeight = FontWeight::NORMAL;
pub const W_LABEL: FontWeight = FontWeight::MEDIUM;
pub const W_STRONG: FontWeight = FontWeight::SEMIBOLD;

/// The transcript's reading size (cmd-= / cmd-- / cmd-0, every Pane) is a
/// zoom over the whole transcript grid: the answer size is the setting's
/// own px.
pub fn answer_text_size(size: ferrite_core::settings::ReadingSize) -> f32 {
    f32::from(size.px())
}

/// The pixel line height paired with each reading size: 1.5× and whole
/// (`prose_line_height`): 12/18 · 13/20 · 14/21 · 15/23 · 16/24 · 18/27 ·
/// 20/30 · 22/33 · 24/36.
pub fn answer_line_height(size: ferrite_core::settings::ReadingSize) -> f32 {
    prose_line_height(answer_text_size(size))
}

/// How much smaller a Group Pane's transcript is set than the reading
/// size: none — a Group Pane reads at the base size (14/21 at Standard).
pub const TX_GROUP_STEP: f32 = 0.0;
/// How much larger a Solo transcript is set than the reading size: none —
/// 14/21 at Standard. The references measure there: Codex sets 14px SF on
/// a 22 pitch, and Claude's larger non-SF face matches SF 14's x-height on
/// a 21 pitch (`docs/research/craft/typecolour.md`).
pub const TX_SOLO_STEP: f32 = 0.0;
/// How much smaller a diff or a code fence is set than the transcript
/// around it: one step of the scale (13 at Standard).
pub const TX_BLOCK_STEP: f32 = 1.0;

/// A transcript's text size: `TX_SOLO_STEP` over the reading size in
/// Solo, `TX_GROUP_STEP` under it in a Group Pane.
pub fn tx_size(size: ferrite_core::settings::ReadingSize, solo: bool) -> f32 {
    answer_text_size(size) + if solo { TX_SOLO_STEP } else { -TX_GROUP_STEP }
}

/// Headings are the body size (rule 1): they stand apart by weight and ink.
/// Kept as a function so the Markdown path has one place to ask.
pub fn heading_scale(_level: u8) -> f32 {
    1.
}

/// The transcript's line over its size: 1.5 (14/21), Claude's measured
/// pitch over SF 14; Codex's 14/22 is a hair looser.
pub const TX_LINE_RATIO: f32 = 1.5;

/// A transcript size's pixel line height: `round(size × 1.5)`.
pub fn prose_line_height(size: f32) -> f32 {
    (size * TX_LINE_RATIO).round()
}

/// 0.6em — Geist Mono's advance width (600/1000 em): every glyph's cell.
pub const CODE_ADVANCE: f32 = 0.6;
/// 7.8px — one cell of Geist Mono at 13px, the grid's unit. The cell is a
/// spacing unit now, not a type size: it stayed at 13 when chrome type
/// went to 14, so no layout moved with the type.
pub const CODE_CELL: f32 = 13.0 * CODE_ADVANCE;
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
/// The glyphs the design uses that Geist Mono lacks: each is typed all the
/// same, and the platform's fallback face sets it (rule 10). Asserted
/// *absent* from the face, so a face update that adds one is noticed.
#[cfg(any(test, feature = "test-support"))]
pub const FALLBACK_GLYPHS: &[char] = &[
    '❯', '✻', '◆', '⏵', '✓', '✗', '▾', '▸', '⌘', '⌥', '⌃', '⎿', '∴', '☐', '⎇', '■', '✢', '✳', '✶',
    '✽',
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
// **Three radii** (rule 2, the Claude app's family: 6 · 10 · 12), and round
// for dots, pills and the send circle (`rounded_full`).

/// 6px — controls and rows: nav and menu rows, buttons, icon buttons,
/// tabs, chips, inline code, fields, the prompt band, diff and fence
/// blocks, a Decision option. The kit's `radius`.
pub const R_CONTROL: f32 = 6.0;
/// 10px — cards: the user's bubble, a Group's Pane, the Decision box, a
/// tool or comment card, the failure strip, the panel's file block, a sent
/// image's thumbnail.
pub const R_CARD: f32 = 10.0;
/// 12px — the Composer and every float: menus, popovers, the palette,
/// sheets. The kit's `radius_lg`.
pub const R_FLOAT: f32 = 12.0;
/// 2px — meter segments and other tiny marks.
pub const R_TIGHT: f32 = 2.0;

// --------------------------------------------------------- shell and board

/// 256px — the navigation column (SPEC §5.1); folded it is the rail.
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input. Off the space scale: Claude's sidebar
/// (geometry.md "Sidebar width").
pub const NAV_WIDTH: f32 = 256.0;
/// 44px — the head row (SPEC §4), ADR 0014's titlebar grown into the row
/// that holds the traffic lights over the nav, the view head over the board
/// and the panel's tabs over the panel. The host traffic lights centre here
/// (`TRAFFIC_Y`), the row drags the window outside its controls, and the
/// board's cards start below it (`titlebar::board_top`). Off the space
/// scale: a 28px control and 8px of frame either side, closed by the
/// plane's `LINE` hairline (geometry.md "Title bar height").
pub const WIN_CHROME_H: f32 = TITLEBAR_H;
/// 44px — the head row, as Claude's and Codex's title bars. Off the scale.
pub const TITLEBAR_H: f32 = 44.0;
/// The view head in the head row over the board (SPEC §5.2): padded
/// `VIEW_HEAD_PAD_X` (20, geometry.md "Title text x") inside the board's column at both ends (the
/// title over the cards' left edge, `New thread` over their right, clear
/// of the panel's tabs), its parts
/// `VIEW_HEAD_GAP` (10) apart — the title (`FS_TITLE` 600), the Project in
/// `INK_SUBTLE`, a spacer, then `New thread` (Group) and, while the panel is
/// hidden, the panel toggle. A secondary button is `CONTROL_H` at
/// `R_CONTROL` on `RAISED`, padded `CONTROL_CHIP_PAD_X` (10), its icon
/// and label `CONTROL_CHIP_GAP` (6) apart, the icon `CONTROL_CHIP_ICON`
/// (13). Off the scale: the mockups' head.
pub const VIEW_HEAD_PAD_X: f32 = 20.0;
pub const VIEW_HEAD_GAP: f32 = 10.0;
/// The history arrows (Claude's `‹ ›`, claude/answer-done.png x 126 and
/// 153), at `HISTORY_X` in the head row: `ICON_BUTTON` doors edge to edge, a 28pt pitch, the glyph
/// `NAV_TOGGLE_GLYPH`. At an end the arrow is disabled in `INK_SUBTLE`:
/// Claude's disabled arrow is 0.6 of its live one (#7a777a by #ccc9cb), as
/// `INK_SUBTLE` is of the doors' `INK_STRONG`.
pub const HISTORY_ARROW_DISABLED_INK: u32 = INK_SUBTLE;
/// The title-bar tools at the row's right end (Claude's panel, terminal,
/// diff, preview, files, `⋯`): `ICON_BUTTON` doors 4 apart, Claude's 32pt
/// pitch (claude/answer-done.png x 1738–1898), the panel toggle the last.
pub const TITLE_TOOLS_GAP: f32 = SPACE_1;
pub const CONTROL_CHIP_PAD_X: f32 = 10.0;
pub const CONTROL_CHIP_GAP: f32 = SPACE_1_5;
pub const CONTROL_CHIP_ICON: f32 = 14.0;
/// Where the host traffic-light group's close button sits: 13px in from the
/// window's left edge, vertically centred for a 14px button in the band.
pub const TRAFFIC_X: f32 = 13.0;
pub const TRAFFIC_Y: f32 = (WIN_CHROME_H - 14.0) / 2.0;
/// The traffic-light group's width: three 14px buttons 6 apart.
pub const TRAFFIC_W: f32 = 3.0 * 14.0 + 2.0 * 6.0;
/// The history arrows' left edge, from the window's: `SPACE_2` past the
/// traffic lights, where Claude and Codex keep theirs — the sidebar head's
/// empty left slot while it is open, the same spot over the rail folded.
pub const HISTORY_X: f32 = TRAFFIC_X + TRAFFIC_W + SPACE_2;
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

/// 320px — the right panel (SPEC §5.6), a fixed column on the frame
/// outside the board, Claude's: 1 hairline and 319; padding 16 either side
/// (`geometry.md`), 8 under it.
pub const PANEL_W: f32 = 320.0;
pub const PANEL_PAD_L: f32 = SPACE_4;
pub const PANEL_PAD_R: f32 = SPACE_4;
pub const PANEL_PAD_B: f32 = SPACE_2;
/// The panel body's scrollbar lies in the column's right padding
/// (`PANEL_PAD_R`), beside the rows rather than over them, as the nav's
/// does: a 16px hit strip, the 6px hovered thumb 5px from either side.
pub const PANEL_SCROLLBAR_INSET: f32 = 5.0;
const _: () = assert!(PANEL_PAD_R == 2. * PANEL_SCROLLBAR_INSET + SCROLLBAR_THUMB_W_HOVER);
/// 28px — a panel tab in the head row (`Overview` and a preview's), 10px
/// inline padding; 2px between tabs.
pub const PANEL_TAB_H: f32 = 28.0;
pub const PANEL_TAB_PAD_X: f32 = 10.0;
/// The Preview tab's label, the file's name cut from its start, is never
/// wider than this: the tabs beside it keep their places.
pub const PANEL_PREVIEW_TAB_MAX_W: f32 = 160.0;
/// The Panel while its Preview tab shows (SPEC §5.6.3): `PANEL_PREVIEW_SHARE`
/// of the window, held between `PANEL_PREVIEW_MIN_W` and
/// `PANEL_PREVIEW_MAX_W` — wide enough to read code at its own size — and
/// never so wide that the board beside it and the full nav column is
/// narrower than `PANEL_PREVIEW_BOARD_MIN`: what the nav's auto-rail asks
/// of the board (rule 2.7.7), so opening a preview never folds the nav and
/// the Pane beside it stays a readable Pane. Never narrower than the Panel
/// itself (`PANEL_W`): a narrow window previews at the Panel's width.
pub const PANEL_PREVIEW_SHARE: f32 = 0.42;
pub const PANEL_PREVIEW_MIN_W: f32 = 420.0;
pub const PANEL_PREVIEW_MAX_W: f32 = 820.0;
pub const PANEL_PREVIEW_BOARD_MIN: f32 = NAV_AUTO_RAIL_BOARD_W + NAV_AUTO_RAIL_HYSTERESIS;
/// A panel row's 14px icon on the content edge (x 0), its text at x 30,
/// Claude's (`geometry.md`: icon x1619, text x1647 off a 1617 edge). A file
/// block's head is a two-line row (48), its icon at y 9, the whole row
/// inset `PANEL_CARD_PAD_X` in its `RAISED` card.
pub const PANEL_ROW_H: f32 = 48.0;
pub const PANEL_ROW_ICON: f32 = 16.0;
pub const PANEL_ROW_ICON_X: f32 = 0.0;
pub const PANEL_ROW_ICON_Y: f32 = 9.0;
pub const PANEL_ROW_TEXT_X: f32 = 30.0;
pub const PANEL_CARD_PAD_X: f32 = SPACE_2;
/// 32px — a panel line (a section head, an Environment row, a connector,
/// `N files changed`): Claude's row pitch.
pub const PANEL_SECTION_H: f32 = 32.0;
/// 16px — between two panel sections, space and no rule (Claude's: a
/// section's last row to the next head steps the row pitch plus 16,
/// claude/side-panel.png).
pub const PANEL_SECTION_GAP: f32 = SPACE_4;
/// 32px — the panel's full-width secondary buttons (`Review changes`).
pub const PANEL_BUTTON_H: f32 = 32.0;
/// 24px — a button at the end of a panel line (`Create PR`, Claude's: 77×24
/// at the right of the Changes line), its words `PANEL_LINE_BUTTON_PAD_X`
/// in from its sides.
pub const PANEL_LINE_BUTTON_H: f32 = 24.0;
pub const PANEL_LINE_BUTTON_PAD_X: f32 = 10.0;
/// A changed file's actions (copy its path, open it in its app), laid
/// over its diffstat while the pointer is on its head: 22px doors (the
/// nav heading's `+`) holding a 13px glyph, 2px apart, on the head's own
/// `RAISED`, a door's hover a step up (`SELECTED`).
pub const PANEL_FILE_ACTION: f32 = NAV_HEADING_BUTTON;
pub const PANEL_FILE_ACTION_GLYPH: f32 = 14.0;
/// A file block's hunk area (phase 7b): on `BLOCK`, padding 4 0 6, its lines
/// in the panel's diff face (`FS_PANEL_DIFF`/`LH_PANEL_DIFF`), each on a
/// 2.5-cell sign column inset 1 cell; no line numbers.
pub const PANEL_HUNK_PAD_T: f32 = SPACE_1;
pub const PANEL_HUNK_PAD_B: f32 = SPACE_1_5;
pub const PANEL_HUNK_INSET_CELLS: f32 = 1.0;
pub const PANEL_HUNK_SIGN_CELLS: f32 = 2.5;

// ------------------------------------------------------------ checks

/// 15px — a CI check's mark wherever one is drawn (`components::CheckMark`:
/// the checks card); a queued
/// or skipped check's hollow ring is 11 inside it, a skipped one's crossed
/// by a `CHECK_SKIP_DASH` dash (off the scale: the ring's inner width less
/// a pixel each side).
pub const CHECK_MARK: f32 = 15.0;
pub const CHECK_MARK_RING: f32 = 11.0;
pub const CHECK_SKIP_DASH: f32 = 5.0;

/// A toast's widest, less where the board is narrower
/// (`cockpit::toast_float`).
pub const TOAST_W: f32 = 320.0;
/// The gap the kit fans a stack out by under the pointer. The stack holds
/// one toast (`max_items`), so this is only ever the kit's own spacing.
pub const TOAST_GAP: f32 = SPACE_3;

// -------------------------------------------- pane body and the row column

/// 16px — the inline padding every Pane strip shares.
pub const PANE_PAD_X: f32 = SPACE_4;
/// The Pane body's padding: 16px top, so the first line never kisses the
/// head rule, and half a row (10px) at the bottom (the prototype's `.tx`
/// `padding-bottom: calc(var(--lh)/2)`): an idle followed tail ends 10px
/// above the Composer's rule. The working line is the transcript's last
/// row while a turn runs (`pane::working_tail`), under the newest output.
pub const BODY_PAD_T: f32 = SPACE_4;
pub const BODY_PAD_B: f32 = LH_UI / 2.0;
/// A Group Pane's body ends 14px over its compact Composer (SPEC §5.9, the
/// mockup's `padding-bottom: 14px`), so a Decision or failure strip at the
/// tail sits where it is drawn.
pub const BODY_PAD_B_GROUP: f32 = 14.0;
/// The working line's down arrow before its token count: Lucide's
/// `arrow-down` in the line's ink, a step under its text (`pane::working`).
pub const WORKING_ARROW: f32 = 12.0;
/// A long thread's date divider: the date's hairlines stand this far from
/// its words (`transcript::date_divider`).
pub const DATE_DIVIDER_GAP: f32 = SPACE_3;

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
/// 16px — an icon button's glyph, centred in its door (Claude's 16).
pub const ICON_BUTTON_GLYPH: f32 = 16.0;
/// 12px — the chevron beside a picker or a disclosure.
pub const ICON_CHEVRON: f32 = 12.0;
/// 28px — a text button in pane and nav chrome, decisions and footers, with
/// 12px inline padding. Sheet controls are `FORM_CONTROL_H`.
pub const CONTROL_H: f32 = 28.0;
pub const CONTROL_PAD_X: f32 = SPACE_3;
/// **The float's shell** (SPEC §5.8 "Other floats": the palette's at a
/// smaller scale): radius `R_FLOAT`, rows `FLOAT_ROW_H` (`LH_UI` and 4px
/// above and below, Claude's 28), each inset `MENU_ROW_INSET_X` from the float's sides
/// (`FLOAT_PAD` above and below the rows) with its text `MENU_ROW_PAD_X`
/// inside its fill, so a line without a fill (a head, a note, a footer, a
/// divider's ends) sets its text `FLOAT_PAD_X` in: 16, Claude's model and
/// permission menus (claude/model-menu.png: text x1053 in a menu at
/// x1037; hover fill 6 in from the menu's side, claude/effort-menu.png).
pub const FLOAT_ROW_H: f32 = 28.0;
/// A row with a one-line note under its label (a mode's description): the
/// label's line and the note's meta line in 5px of air above and below,
/// Claude's 46.
pub const FLOAT_ROW_NOTE_H: f32 = LH_UI + LH_META + 2.0 * 5.0;
/// A row's tag (`Recommended`, `Default`): Claude's ring, no fill — a word
/// set apart, not a button (claude/permission-menu.png).
pub const TAG_RING: u32 = 0x595959;
pub const FLOAT_PAD: f32 = SPACE_1;
pub const FLOAT_PAD_X: f32 = MENU_ROW_INSET_X + MENU_ROW_PAD_X;
/// The gap a float keeps from the control that opened it.
pub const FLOAT_OFFSET: f32 = 6.0;
/// The context menu's least width (it grows to its longest verb beside
/// its key: Claude's row menu is 165, claude/row-menu.png), and any
/// floating list's height cap before it scrolls.
pub const MENU_W: f32 = 160.0;
pub const MENU_MAX_H: f32 = 420.0;
/// A menu row's side inset, its inline padding and the gap between its
/// label and trailing parts; its fill's radius is `R_CONTROL`, inside the
/// float's 12 at a 6px inset.
pub const MENU_ROW_INSET_X: f32 = SPACE_1_5;
pub const MENU_ROW_PAD_X: f32 = 10.0;
pub const MENU_ROW_GAP: f32 = SPACE_3;
/// A section heading: 24px, its 13/500 word sat on the heading's foot
/// (4px under it) so it hugs the rows it heads, as the palette's does.
/// A heading that opens a float puts its word's cap 12 under the float's
/// top, Claude's 11 (claude/permission-menu.png `Mode`; 28 drew it 16).
pub const FLOAT_SECTION_H: f32 = 24.0;
/// 8px — what splits one group of rows from the next inside a floating
/// surface. Space, never a rule — but a context menu's groups take
/// Claude's hairline (`menu::rule`).
pub const MENU_GROUP_GAP: f32 = SPACE_2;
/// 4px — the air either side of a context menu's group rule
/// (`menu::rule`), Claude's.
pub const MENU_RULE_GAP: f32 = SPACE_1;
/// 4px — a row's `⋯` menu hangs this far under the button (Claude's).
pub const MENU_UNDER_GAP: f32 = SPACE_1;
/// 16px — a row's leading mark (a menu item's icon, a dragged Pane's file
/// mark), Claude's: as big as the name beside it, in the name's ink.
pub const ROW_ICON: f32 = 16.0;

// ------------------------------------------------------- status and motion

/// The room a typed status dot (`components::status_dot`) takes in a row:
/// its one cell.
pub const STATUS_DOT: f32 = CH;

// ------------------------------------------------------------------ faces

/// The face of everything that is read or touched (rule 1): **the
/// system's UI font** — SF Pro on macOS, Segoe UI on Windows — prose,
/// nav, heads, tabs, buttons, menus, the palette, the panel, the bar.
/// gpui's `.SystemUIFont` names it on every platform. The kit's
/// `font_family` is set from it.
pub const FONT_UI: &str = ".SystemUIFont";

/// The code face, **bundled** (`lib.rs`): code, diffs, commands, machine
/// output and terminal numbers anywhere — Geist Mono, the kit's
/// `mono_font_family`. Its static instances share one family name, and
/// CoreText/DirectWrite resolve the instance from `.font_weight(..)`:
/// **never reach a weight by family name**.
pub const FONT_CODE: &str = "Geist Mono";

/// Install Longbridge once per app, then map its semantic theme to Ferrite's
/// tokens. Constructors also call this for standalone test windows. The
/// window mounts the toolkit Root for Settings search input and focus.
pub fn init_components(cx: &mut gpui::App) {
    use gpui::component::{Theme, ThemeMode};
    use gpui::{px, rgb, rgba};

    if cx.has_global::<Theme>() {
        return;
    }
    gpui::component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    // Two faces (rule 1): the system UI font, Geist Mono for code.
    theme.font_family = FONT_UI.into();
    theme.font_size = px(FS_UI);
    theme.mono_font_family = FONT_CODE.into();
    theme.mono_font_size = px(FS_UI);
    // Radii by role (rule 2).
    theme.radius = px(R_CONTROL);
    theme.radius_lg = px(R_FLOAT);
    // Floats carry Ferrite's one float shadow; the kit's own stays off.
    theme.shadow = false;
    theme.motion.spring_move = gpui::base::Spring::new(std::time::Duration::from_millis(120))
        .with_damping(1.0)
        .with_epsilon(0.1);
    // The kit's ground is the card (the window Root's own ground is
    // `paint::WINDOW`, `lib.rs`).
    theme.background = rgb(CARD).into();
    theme.foreground = rgb(INK_STRONG).into();
    theme.border = paint::LINE.into();
    // Kit menu and completion rows sit on a float: their cursor face is
    // the float's cursor step, their hover its hover step.
    theme.accent = paint::FLOAT_SEL.into();
    theme.accent_foreground = rgb(INK_STRONG).into();
    theme.secondary = rgb(RAISED).into();
    theme.secondary_hover = rgb(SELECTED).into();
    theme.secondary_active = rgb(SELECTION_HOVER).into();
    theme.secondary_foreground = rgb(INK_STRONG).into();
    // The primary has no hue: the strongest ink, stepping down to `PRIMARY_HOVER`
    // under the pointer and pressed.
    theme.primary = rgb(PRIMARY).into();
    theme.primary_hover = rgb(PRIMARY_HOVER).into();
    theme.primary_active = rgb(PRIMARY_HOVER).into();
    theme.primary_foreground = rgb(ON_PRIMARY).into();
    // `Button::primary` reads its own fields, not `primary`.
    theme.button_primary = rgb(PRIMARY).into();
    theme.button_primary_hover = rgb(PRIMARY_HOVER).into();
    theme.button_primary_active = rgb(PRIMARY_HOVER).into();
    theme.button_primary_foreground = rgb(ON_PRIMARY).into();
    theme.muted = rgb(RAISED).into();
    theme.muted_foreground = rgb(INK_SUBTLE).into();
    theme.popover = paint::FLOAT.into();
    theme.popover_foreground = rgb(INK_STRONG).into();
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
    theme.switch_thumb = rgb(INK_STRONG).into();
    theme.overlay = rgba(VEIL).into();
    theme.danger = rgb(BLOCKED).into();
    theme.warning = rgb(ATTENTION).into();
    theme.success = rgb(RUNNING).into();
    theme.info = rgb(INK_STRONG).into();
    theme.list_hover = paint::FLOAT_HOVER.into();
    theme.list_active = paint::FLOAT_SEL.into();
    theme.table_head = rgb(CARD).into();
    theme.table_head_foreground = rgb(INK_STRONG).into();
    theme.drag_border = rgba(DROP_EDGE).into();
    theme.drop_target = rgba(DROP_WASH).into();
    theme.sidebar = rgb(SIDEBAR).into();
    theme.sidebar_foreground = rgb(INK_STRONG).into();
    // The selected Settings page is the selection row.
    theme.sidebar_accent = rgb(SELECTED).into();
    theme.sidebar_accent_foreground = rgb(INK_STRONG).into();
    theme.sidebar_border = rgba(TRANSPARENT).into();
    // No track: only the thumb is ever ink, and it lightens rather than
    // darkens when the pointer takes hold of it.
    theme.scrollbar = rgba(TRANSPARENT).into();
    theme.scrollbar_thumb = rgb(SCROLLBAR).into();
    theme.scrollbar_thumb_hover = rgb(SCROLLBAR_HOVER).into();
    // Kit tabs, should one appear: no bar ground, the active tab `SELECTED`
    // in `INK_STRONG`, the rest muted.
    theme.tab_bar = rgba(TRANSPARENT).into();
    theme.tab_active = rgb(SELECTED).into();
    theme.tab_active_foreground = rgb(INK_STRONG).into();
    theme.tab_foreground = rgb(INK_SUBTLE).into();
    // `Theme::change` resolved the kit's pre-computed tokens from its default
    // palette, and nothing recomputes them: widgets that read `tokens.*`
    // (Button::primary, menu rows, tooltips, checkboxes) would paint the
    // kit's neutrals. Rebuild them from the colours above.
    theme.tokens = gpui::component::ThemeTokens::from(&theme.colors);
    // A toast is the rail's voice only (C8): with the nav open its rows'
    // marks are the queue, so the Needs-you toast draws only while the nav
    // is railed (`cockpit::toast_float`) and its Thread is off the board
    // (the ring's `in_view`); its sound and system notification ring either
    // way. Every toast, the kit's and the Needs-you one, stands TopRight
    // (`TOAST_*`), clear of the Composer the operator types in, one at a
    // time; the Inbox holds the rest.
    theme.notification.placement = gpui::Anchor::TopRight;
    theme.notification.margins = gpui::base::Edges {
        top: px(WIN_CHROME_H + PANE_HEAD_H + TOAST_TOP),
        right: px(TOAST_BOX_RIGHT),
        bottom: px(TOAST_TOP),
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
    // Ferrite's timing — on the first scroll frame, a 1.4s hold, a 100ms
    // fade — so they match `scrollbar.rs`.
    let base = gpui::base::Theme::global_mut(cx);
    base.scrollbar = base.scrollbar.clone().with_motion(
        gpui::base::ScrollbarMotion::default()
            .with_enter(std::time::Duration::ZERO)
            .with_idle(std::time::Duration::from_millis(MOTION_SCROLLBAR_LINGER_MS))
            .with_exit(std::time::Duration::from_millis(MOTION_SCROLLBAR_FADE_MS)),
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
// - **Marks.** `❯` (`INK_STRONG`, semibold) heads the operator's prompt; `●`
//   heads agent prose (`INK_STRONG`, once per speaker change:
//   `transcript::AnswerMarks`) and every tool call (`INK_SUBTLE` done,
//   `RUNNING` live, `BLOCKED` failed; never pulsing); `✻` (in the
//   provider's colour) the turn's end; `◆` (`ATTENTION`) a Decision — all
//   typed (rule 10). `∴` (drawn) heads reasoning.
// - **No banner.** A transcript opens on its first prompt: the head names
//   the Thread and its Project, the Panel its branch (ADR 0014's banner is
//   retired).
// - **The prompt bubble.** The prompt echo is a right-aligned
//   `paint::PROMPT_BUBBLE` bubble (`R_CARD`) shrinking to its words in SF Pro
//   `INK_STRONG`, its non-image files' chips inline after them, its
//   images as thumbnails under it; its send time and `Copy` left of it
//   while the pointer is on it (the message actions). A turn's answer
//   carries its `Copy` under it the same way.
// - **Reading order.** Rows read down from the top of the body, as a new
//   conversation does; the tail is followed once they overflow, and a
//   reader scrolled away from it gets `Jump to latest`.
// - **The pinned prompt.** The prompt of the turn being read stays pinned at
//   the top of the body while its output scrolls under it: its own bubble,
//   right-aligned, clamped to `PINNED_PROMPT_LINES`, on a plane in the
//   card's fill (`PINNED_PROMPT_PAD_Y` above and below), the rows passing
//   under it into the top fade (`EDGE_FADE`); pushed up by the next
//   turn's prompt. A click on it jumps back to its turn.
// - **Activity.** A call or a run of calls is one muted line in the app's
//   words (`Edited 3 files ›`, `Ran cargo test · 954 passed ›`), a literal
//   a step brighter on no chip; no icon, no time, its `›` when it opens.
//   Opened, its input sits in a quiet well, its output plain under it.
//   The agent's narration between calls reads in the same muted ink; only
//   a turn's last prose reads bright.
// - **Rhythm in rows.** A turn's work reads as one list, its rows half a
//   line apart; every other block (a prompt, an answer, the stamp) stands
//   a line and a half off; a row that answers the one above it sits flush.
//   The space above a row is chosen at reconcile from the row before it
//   (`rows::gap_before`) and is part of the row's identity.
// - **The turn's end.** A turn that took `TURN_STAMP_MIN_MS` or more closes
//   on `Worked for 5m 22s`, `INK_SUBTLE`, a `LINE` running on to the
//   column's end; a quicker one says nothing. A failed or interrupted turn
//   says `failed · 0.1s · <message>`, the lead word in its state ink.
// - **Failure** is a word: a failed call's line stays in the work's ink and
//   its verdict (`· failed`) follows the cut, `failed` in `BLOCKED`; what
//   it printed reads as output does.
// - **Diffs** sit in a hairline well (`pane::diff_well`: no fill, `R_CONTROL`,
//   an inset `CARD_RING`), `BLOCK_MARGIN_T` under their call: a `INK_SUBTLE`
//   hunk header naming its section (none when it has none), line numbers
//   right-aligned in `DIFF_NUMBER_CELLS` (`TEXT_FAINT`, or
//   `DIFF_*_NUMBER` on a changed row), a `DIFF_SIGN_CELLS` sign carrying
//   the hue, red and green row washes (`DIFF_*_WASH`) with the changed
//   tokens washed deeper (`DIFF_*_WORD`), code in full syntax colour and
//   one line per row, cut with `…`, always unified under a header naming
//   the file and its size, shown once its call is opened.
// - **No transcript row has a hover ground** but the keyboard's disclosure
//   target (`paint::RAISED`).

/// A prose-relative vertical step at answer size `size`: em-proportional to
/// the Standard prose size, whole pixels. `GAP_TURN`, `GAP_BLOCK` and the
/// Markdown gaps go through it; UI-row steps do not.
#[cfg(any(test, feature = "test-support"))] // the previous grammar, now read only by tests
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
/// **The tool group's card** (`pane::render_tool_group`, Claude's measured
/// card, geometry.md "Tool group (open)"): each call's row
/// `TOOL_CARD_ROW_H` for its line, its words `TOOL_CARD_INSET` in from the
/// card's edges, the card hanging `CARD_OUTSET` past the column's left
/// edge so its rows' words sit a step inside the prose.
pub const TOOL_CARD_ROW_H: f32 = 36.0;
pub const TOOL_CARD_INSET: f32 = SPACE_3;
/// 4px — **one left edge** (Claude's): text sits on the transcript column
/// and every card's ring hangs this far outside it — the tool group, the
/// task list, a fence, a Decision.
pub const CARD_OUTSET: f32 = SPACE_1;
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

/// **The transcript's turn rhythm** at a line of `line` (21 at Standard),
/// as Claude spaces it (`docs/research/craft/geometry.md`), scaling with the
/// reading size: 40 from a prompt's bubble to whatever answers it, 22
/// between a turn's prose and its calls (either way), 56 from a turn's last
/// block to the next prompt.
pub fn tx_after_prompt(line: f32) -> f32 {
    (line * 40. / 21.).round()
}
pub fn tx_work_prose(line: f32) -> f32 {
    (line * 22. / 21.).round()
}
pub fn tx_turn_gap(line: f32) -> f32 {
    (line * 56. / 21.).round()
}
/// The space between an answer's paragraphs (and its other blocks): 12 on
/// the 21 line, Claude's 11 rounded to the grid, so an answer reads as one
/// piece rather than as terminal output with a blank line between blocks.
pub fn tx_paragraph_gap(line: f32) -> f32 {
    (line * 12. / 21.).round()
}
/// A tight list's items stand this much further apart than their lines
/// (Claude's 24 pitch on a 21 line).
pub const TX_LIST_ITEM_GAP: f32 = 3.0;

/// The transcript column's inset from the card's edges, left and right
/// (SPEC §5.4): 28px in Solo, 18px in a Group Pane. Off the space scale:
/// the mockups' `padding: 0 28px` / `0 18px`. The right inset clears the
/// minimap rail (`MINIMAP_W`).
pub const TX_INSET_SOLO: f32 = 28.0;
pub const TX_INSET_GROUP: f32 = 18.0;

/// The reading column's widest: the transcript centres in the card with
/// this at most between its edges (Codex's 736 column; Claude's is 732),
/// the insets the least room either side.
pub const TX_COLUMN_MAX_W: f32 = 736.0;

/// The reading column's width in a transcript `width` wide: the width less
/// the insets, at most `TX_COLUMN_MAX_W`.
pub fn tx_column(width: f32, solo: bool) -> f32 {
    (width - 2. * tx_inset(solo)).min(TX_COLUMN_MAX_W)
}

/// The transcript column's inset for a Solo (`solo`) or a Group Pane.
pub fn tx_inset(solo: bool) -> f32 {
    if solo {
        TX_INSET_SOLO
    } else {
        TX_INSET_GROUP
    }
}

/// **The prompt bubble** (the prototype's `.bubble`): right-aligned,
/// shrinking to its words, never wider than `PROMPT_BUBBLE_MAX_W` of the
/// column; `paint::PROMPT_BUBBLE` at `R_CARD`, `PROMPT_BUBBLE_PAD_X` either side
/// and `PROMPT_BUBBLE_PAD_Y` above and below; `PROMPT_BUBBLE_GAP` between
/// it, its thumbnails and its send time.
pub const PROMPT_BUBBLE_MAX_W: f32 = 0.84;
pub const PROMPT_BUBBLE_PAD_X: f32 = SPACE_4;
pub const PROMPT_BUBBLE_PAD_Y: f32 = 10.0;
pub const PROMPT_BUBBLE_GAP: f32 = SPACE_1_5;

/// **A sent image's thumbnail** (Claude's, over its bubble): the picture
/// at its own proportions, no larger than `THUMB_MAX_W` × `THUMB_MAX_H`
/// (nor its own size), at `R_CARD` in a 1px `LINE` ring,
/// `PROMPT_IMAGE_GAP` above the bubble. claude/plus-menu.png: a 16:10
/// screenshot at 480 × 269, 10 above its bubble, right edges level.
pub const THUMB_MAX_W: f32 = 480.0;
pub const THUMB_MAX_H: f32 = 320.0;
pub const PROMPT_IMAGE_GAP: f32 = 10.0;

/// **The block box** — a code fence's (SPEC §5.4; a diff's is a hairline well):
/// `paint::BLOCK` ground, `R_CONTROL` corners, an inset 1px `BLOCK_RING`, `BLOCK_PAD_Y`
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
/// A diff's line-number column, in cells (its 1-cell pad included), and its
/// sign column.
pub const DIFF_NUMBER_CELLS: f32 = 5.0;
pub const DIFF_SIGN_CELLS: f32 = 2.0;
/// An answer's image frame in cells, its 1px `paint::LINE` border
/// included (the prototype's `.img{width:48ch}`, 374.4px at Standard); the
/// picture fills it.
pub const IMAGE_CELLS: f32 = 48.0;

/// **The minimap** (the transcript's scrollbar): a `MINIMAP_W` rail at the
/// body's right edge, shown while the pointer is anywhere on the Pane
/// (Composer included) and fading over `MOTION_HOVER_FADE_MS`. A
/// `MINIMAP_TICK_H` tick (`MINIMAP_TICK_R` corners) per prompt (`INK_STRONG`),
/// failed call (`BLOCKED`), Decision (`ATTENTION`) and passing check
/// (`RUNNING`), inset `MINIMAP_TICK_INSET` — 6px wide; the viewport as a
/// white band at 6% (`MINIMAP_BAND`), 11% under the pointer
/// (`MINIMAP_BAND_HOVER`), `MINIMAP_BAND_R` corners, inset
/// `MINIMAP_BAND_INSET`, never shorter than `MINIMAP_BAND_MIN_H`; the band
/// hides while the whole transcript fits, the ticks stay. A click on a tick
/// jumps to its row in one frame; anywhere else centres the view on that
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

/// The room over a transcript's first row: a Solo Pane's `TX_PAD_T`, a
/// board Pane's `TX_PAD_T_BOARD`, so a first prompt's bubble never meets
/// the head's rule.
pub const TX_PAD_T: f32 = LH_UI;
pub const TX_PAD_T_BOARD: f32 = SPACE_3;
/// What a prompt band's chip line adds below it: the prototype's chip is an
/// `inline-flex` box seated on its icon's synthesized baseline, which lifts
/// it above the line's strut and grows that line box by two pixels. The
/// chips close the prompt, so the band takes the extra under its last line.
pub const PROMPT_CHIP_LINE_EXTRA: f32 = 2.0;
/// A running test's bar (the prototype's `.bar`): `TEST_BAR_CELLS` wide,
/// `TEST_BAR_H` tall on a `paint::LINE` track, `RUNNING` fill at sub-pixel
/// precision easing over `TEST_BAR_EASE_MS`, a cell's margin each side,
/// `TEST_BAR_R` corners.
pub const TEST_BAR_CELLS: f32 = 20.0;
pub const TEST_BAR_H: f32 = 6.0;
pub const TEST_BAR_R: f32 = 1.0;
pub const TEST_BAR_EASE_MS: u64 = 300;
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
// ---- transcript app pass (owner: transcript.rs and its rows)
/// A completed turn closes with `Worked for 5m 22s` only when it took at
/// least this long: a quick answer needs no receipt. An interrupted or
/// failed turn always says so.
pub const TURN_STAMP_MIN_MS: u64 = 30_000;
/// The turn stamp's rule runs from its words to the column's end, this far
/// after them.
pub const TURN_STAMP_RULE_GAP: f32 = SPACE_3;
/// **Message actions**: a prompt's (its send time and `Copy`, under its
/// bubble's right edge) and a turn's answer's (`Copy` and the time it
/// finished, under its left edge), `MESSAGE_ACTIONS_GAP` under the message,
/// laid out at rest so the pointer never moves the transcript. They show
/// under the pointer anywhere on the message, and always under the
/// thread's latest answer (Codex: the last answer is where you act). Each is
/// a `MESSAGE_ACTION` square at `R_CONTROL`, its `MESSAGE_ACTION_GLYPH` icon
/// in `INK_SUBTLE`, the raised hover blend; a copy turns to a check for
/// `COPIED_HOLD_MS`. The time is meta type in `INK_SUBTLE`.
pub const MESSAGE_ACTION: f32 = SPACE_6;
pub const MESSAGE_ACTION_GLYPH: f32 = 14.0;
pub const MESSAGE_ACTIONS_GAP: f32 = SPACE_2;
pub const COPIED_HOLD_MS: u64 = 1_500;
/// **The edge fades**: what scrolls under the top of the transcript (or
/// under its pinned prompt) fades out over this height into the card, so no
/// row ends on a hard cut; while rows run on under the foot (into the
/// Composer, Codex's fade), the foot fades the same way. The nav tree fades at whichever edge still
/// has rows past it, into the frame. A fade is solid over its first half,
/// so a row cut right at the edge is hidden, held near-solid
/// (`EDGE_FADE_KNEE`) over the next quarter, then eases out
/// (`components::edge_fade`).
pub const EDGE_FADE: f32 = SPACE_6;
pub const EDGE_FADE_KNEE: f32 = 0.85;
/// **A turn's cards** (Claude's file and preview cards): under a finished
/// turn's answer, one card for each page it wrote or named and each other
/// file it wrote, `OUTPUT_CARD_GAP` apart, each at most `OUTPUT_CARD_W`
/// wide and `OUTPUT_CARD_H` tall at `R_CARD` on `paint::RAISED` in a 1px
/// `paint::LINE` ring, `OUTPUT_CARD_PAD` in. A page's leads with its
/// thumbnail (`OUTPUT_THUMB_W` by the card's inner height, the globe in
/// `INK_SUBTLE` on `CARD`), a file's with its mark in an
/// `OUTPUT_ICON_TILE` square; then the name in `INK_STRONG` at `W_LABEL`
/// over what it is in `INK_SUBTLE` meta type, and at the right a page's
/// `Open` and `⋯`, a file's `Open in ›`. Its menu is a context menu's.
pub const OUTPUT_CARD_W: f32 = 400.0;
pub const OUTPUT_CARD_H: f32 = 56.0;
pub const OUTPUT_CARD_PAD: f32 = SPACE_2;
pub const OUTPUT_CARD_GAP: f32 = SPACE_2;
pub const OUTPUT_ICON_TILE: f32 = 40.0;
pub const OUTPUT_THUMB_W: f32 = 64.0;
/// **The lightbox** (Claude's): an image over the whole window on
/// `LIGHTBOX_VEIL`, darkening in over `FADE_QUICK`. A bar
/// `LIGHTBOX_BAR_H` tall along its top holds the file's name at the left
/// and its doors at the right — zoom, save, open in its app, × —
/// `ICON_BUTTON` squares in `INK_STRONG`. With more than one image, ← →
/// stand `LIGHTBOX_ARROW` round in `LIGHTBOX_SIDE` gutters, and `1 of N`
/// in `INK_SUBTLE` meta type in a `LIGHTBOX_FOOT_H` foot. The stage is
/// what the bar, the gutters and the foot leave; a press on the dim
/// around it closes the lightbox.
pub const LIGHTBOX_BAR_H: f32 = SPACE_6 * 2.;
pub const LIGHTBOX_SIDE: f32 = SPACE_6 * 2. + SPACE_4;
pub const LIGHTBOX_FOOT_H: f32 = SPACE_6 + SPACE_4;
pub const LIGHTBOX_ARROW: f32 = SPACE_6 + SPACE_3;
/// **The line-end fade** (Claude's sidebar titles): a line too long for its
/// box fades into the ground over its last `LINE_FADE`, where it would
/// otherwise end in "…" (`components::line_fade`).
pub const LINE_FADE: f32 = SPACE_6;
/// The pinned prompt keeps at most this many of its lines, its plane
/// `PINNED_PROMPT_PAD_Y` above and below the bubble.
pub const PINNED_PROMPT_LINES: usize = 2;
pub const PINNED_PROMPT_PAD_Y: f32 = SPACE_2;
/// **Jump to latest**: while the reader is scrolled away from the tail, a
/// `JUMP_LATEST` circle (the plane's fill in a 1px `LINE` ring, no shadow:
/// Codex's flat button) with the down arrow floats centred
/// `JUMP_LATEST_INSET` above the transcript's foot; a click snaps to the
/// tail and follows it again.
pub const JUMP_LATEST: f32 = 32.0;
pub const JUMP_LATEST_INSET: f32 = SPACE_3;
/// **Reply to a selection** (Claude's `SelectionTooltip`): a drag that
/// selects a passage ends with a pill centred over the selection, its foot
/// `SELECTION_REPLY_GAP` above it: the word "Reply" (`FS_UI`, medium) then
/// the `REPLY` arrow at `SELECTION_REPLY_GLYPH`, in `SELECTION_REPLY_INK`,
/// `SELECTION_REPLY_PAD_X` either side and `SELECTION_REPLY_PAD_Y` above
/// and below (Claude's 12 and 6). Claude's ground is black at 80% inside a
/// white-at-10% hairline, lit white at 10% under the pointer: here those
/// composited over the transcript's ground, as opaque greys (rule 4). Its
/// radius is Claude's 8, the one pill off the three-radius family.
pub const SELECTION_REPLY_GAP: f32 = SPACE_2;
pub const SELECTION_REPLY_PAD_X: f32 = SPACE_3;
pub const SELECTION_REPLY_PAD_Y: f32 = SPACE_1_5;
pub const SELECTION_REPLY_GLYPH: f32 = 14.0;
pub const SELECTION_REPLY_RADIUS: f32 = 8.0;
pub const SELECTION_REPLY_FILL: u32 = 0x050505;
pub const SELECTION_REPLY_EDGE: u32 = 0x1e1e1e;
pub const SELECTION_REPLY_INK: u32 = INK_STRONG;
/// **The task list** (a plan the provider works to, over the working line):
/// a card at `R_CARD` on a `CARD_RING`, `TASK_CARD_PAD` inside,
/// `TASK_ROW_GAP` between its steps, each a `TASK_ICON` mark and its words —
/// done `check-circle` dimmed, the current step's working mark, a step
/// to do a dashed ring.
pub const TASK_CARD_PAD: f32 = SPACE_3;
/// 16px — the card's clearance in the flow, above it and down to the
/// working line, so it reads as its own block (Claude's), never as the body
/// of the tool group over it.
pub const TASK_CARD_GAP: f32 = SPACE_4;
pub const TASK_ROW_GAP: f32 = SPACE_1_5;
pub const TASK_ICON: f32 = 14.0;
// ---- (end transcript app pass)
// (end WP-A) — append above this line only

// ======================================== WP-B · markdown, prose, scrollbars
// Owner: WP-B (rich.rs, scrollbar.rs, attachments::inline_file, the Markdown vendor knobs.)
// Edit values and append tokens only inside this section.

/// **Markdown** (SPEC §5.4). Agent prose is the one face in `INK_STRONG`
/// at the transcript's size, left-aligned on the content column and held to
/// `MEASURE_CH` cells; code, tables and rules keep the whole column. Blocks
/// sit one blank line apart; headings are the body size in `W_STRONG`
/// `INK_STRONG`, `**strong**` the same. Inline code is `INLINE_CODE_INK`
/// on its `INLINE_CODE_CHIP`; links and file paths are `LINK_INK`, underlined
/// at rest (`PATH_UNDERLINE_*` for a path's own run). A list's `•` is `INK_SUBTLE`, hanging in the first
/// of two cells. A fence is the diff's block box (`BLOCK_*`), the code
/// `CODE_PAD_CELLS` in, `TX_BLOCK_STEP` under the prose, in full syntax
/// colour, at most `MEASURE_CH` cells from the content column; its language
/// tag dim on its first line at the right, `Copy` before the tag blending
/// in under the pointer. A table takes its content's width: a `W_STRONG`
/// `INK_STRONG` head over a `paint::LINE` rule, row rules in `paint::LINE`
/// (each row exactly one line), no vertical rules and no box, the first
/// column flush, figures tabular; a column of numbers aligns right (head
/// included) unless the Markdown says otherwise. An image is framed like a
/// prompt's (`IMAGE_CELLS`) with its caption under it. A quote is a 2px
/// `LINE` rule and `INK_SUBTLE`; a thematic break one `LINE`.
///
/// 12px — between Markdown blocks (`SPACE_3`), the transcript's block
/// step. This and the heading spaces are Standard values; other reading
/// sizes scale them with `reading_step`.
#[cfg(any(test, feature = "test-support"))] // the previous grammar, now read only by tests
pub const PROSE_GAP: f32 = SPACE_3;
/// Inline code's ink: `INK_STRONG` on its chip (cyan is retired, ADR
/// 0033). The Markdown path paints both; the plain-text fallback carries
/// the ink alone.
pub const INLINE_CODE_INK: u32 = INK_STRONG;
/// Inline code's ink in a sent prompt: `RED` (`#f3767b`, sampled from
/// claude/running.png's user bubble) on its `PROMPT_CODE_CHIP`, as Claude
/// draws the code an operator quotes.
pub const PROMPT_CODE_INK: u32 = RED;
/// The inline-code chip (`rich::inline_code_wash`): a `paint::INLINE_CODE_CHIP`
/// ground at `R_CONTROL`, with `INLINE_CODE_PAD_X` of room laid out either
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
/// A Quote row (a selection's Reply, over the draft and the sent
/// prompt) shows at most this many of its passage's lines.
pub const QUOTE_ROW_LINES: usize = 2;
/// 4px — a horizontal rule's own margin inside its block, so it sits 16px
/// from its neighbours.
pub const RULE_MARGIN_Y: f32 = SPACE_1;
/// A fence's code sits this many cells (at the fence's size) in from the
/// block's sides. Its actions overlay — the language id, html `Preview`,
/// `Copy`/`Copied` — keeps the same inset at the right and stands on the
/// code's first line (`BLOCK_PAD_Y` down, one code line high), the id set
/// as the code is, on its baseline. The overlay is always laid out (so it
/// never moves the block); the actions show only under the pointer (the
/// 100ms hover blend), while their keys have focus, or while the caret or
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
// **One plane** (theme rule 2, SPEC §4). The window is the nav and the
// board column under the head row (`WIN_CHROME_H`), all the plane's grey,
// parted by the Cockpit's seams (`plane_seams`). The board column starts at the
// nav's seam; its left edge moves with the nav's width when cmd-B folds
// it, at once. A Solo Pane fills it flush, square and
// unringed; a Group's cells stand `BOARD_PAD` in from the bar and the
// window's edge (`board_pad`), each at `R_CARD` with its ring. Cells on a
// board are `BOARD_SEAM` apart; a seam is the gutter's centre line and its
// 9px grab band (`SEAM_GRAB` in the cockpit) that resizes its two sides,
// clamped to 22–78% of the board. A default grid's aligned seams are one
// line that drags as one, and under the pointer and while held the whole
// line draws `SEAM_LINE_W` of `SEAM_HOVER` in the gutter, fading in over
// `MOTION_HOVER_FADE_MS`.
//
// **The Pane.** `paint::CARD`, its content starting at its own edges. On a
// Group board it is a cell at `R_CARD` whose 1px ring is an overlay just
// outside it (the mockups' `box-shadow: 0 0 0 1px`), so a state change
// reflows nothing. By precedence (`pane::PaneEdge`): a waiting Pane
// `WAIT_RING`, focused or not > focused `FOCUS_RING` > at rest
// `CARD_RING`; a Solo Pane is `Flush`: no ring, no radius. Every Pane root is the `pane::PANE_GROUP`
// hover group.
//
// **Solo has no head**: the nav's selected row names the Thread and the
// body starts at the plane's top edge.
//
// **The Group head** (`pane::group_head`, SPEC §5.9) is one 38px row
// (`PANE_HEAD_H`) closed by a `LINE`, on every board Pane: the row's
// mark (`nav::head_mark`), the title at `W_LABEL` — `INK_STRONG` on the
// focused Pane only — its state word in 12 (`needs you` `ATTENTION`,
// `failed` `BLOCKED`, the elapsed time and an unseen `done` `INK_SUBTLE`),
// then the provider's 13px logo and the fill button. No band on the
// focused head: the ring says focus. A draft carries its × instead.
//
// **Two altitudes** (R12): a Pane reads its transcript from 300px wide and
// 360px tall (`docview::Level::for_cell`); smaller, it is a wall tile. One
// Level per board (rule 2.3.5), with `LEVEL_HYSTERESIS`. A tile is the
// head, its state word at the strong weight in its colour, half a row, its
// last `WALL_LINES` lines — one `INK_SUBTLE` run each, glyph included —
// and, while it waits, quick answers boxed in `paint::LINE`; no meter, no
// sparkline.
//
// **The window chrome** (ADR 0014, SPEC §4). The head row is the nav's top
// chrome, the traffic lights' row and the window's drag strip, and the slot
// the view head and the panel's tabs fill; the cards start below it. There
// is no bottom bar: the view switcher sits in the nav's foot.
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

/// 38px — the Group head (SPEC §5.9; a preview's and a Review's heads are
/// its twins, `preview::reading_head`): the
/// card's tab strip height, its `LINE` inside it.
pub const PANE_HEAD_H: f32 = 38.0;
/// A dragged slot's ghost head's inline inset: one cell.
pub const HEAD_PAD_X: f32 = CH;
/// The Group head's insets and gap (and its twins', the reading heads): 14
/// before the mark, 6 after the fill button or a reading head's × (its 28px
/// hit box carries the rest of the visual inset), 8 between items.
pub const PANE_HEAD_PAD_L: f32 = 14.0;
pub const PANE_HEAD_PAD_R: f32 = SPACE_1_5;
pub const PANE_HEAD_GAP: f32 = SPACE_2;
/// 14px — the provider's logo and a mark's icon in the Group head.
pub const PANE_HEAD_ICON: f32 = 14.0;
/// The width a dot mark lays out at in the Group head: the dot and a pixel
/// each side, as the mockup's box.
pub const PANE_HEAD_DOT_W: f32 = NAV_DOT + 2.0;
/// 14px — the fill button's `maximize` glyph.
pub const PANE_HEAD_FILL_GLYPH: f32 = 14.0;

/// The compact Composer (SPEC §5.9 item 5, `pane::compact_composer`): a
/// 38px box `COMPACT_COMPOSER_INSET` in from the card's sides and foot,
/// padded 12 before the input and 7 after the send circle (24, its
/// `arrow-up` 12).
pub const COMPACT_COMPOSER_H: f32 = 38.0;
pub const COMPACT_COMPOSER_INSET: f32 = 10.0;
pub const COMPACT_COMPOSER_PAD_L: f32 = SPACE_3;
/// Off the space scale: centres the 24px circle 7px from the box's edge,
/// as the mockup draws it.
pub const COMPACT_COMPOSER_PAD_R: f32 = 7.0;
pub const SEND_CIRCLE_SM: f32 = SPACE_6;
pub const SEND_GLYPH_SM: f32 = SPACE_3;

/// The failure strip (SPEC §5.9 item 4): 36 high at `R_CARD` on
/// `FAILURE_WASH`, padded 12 / 6, its `x-circle` 14, its `Retry` 26 high
/// at `R_CONTROL`, padded 10.
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
/// How much readier a Group head's title is to shrink than its state word
/// (flex shrink factors): the title ellipsizes to its floor first.
pub const HEAD_TITLE_SHRINK: f32 = 64.0;
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

/// A file's numbered lines in the Panel's preview (`cockpit/panel/
/// preview.rs`): line numbers in a 6-cell column, right-aligned, two cells
/// before the code.
pub const READER_NUMBER_W: f32 = 6.0 * CH;
pub const READER_NUMBER_GAP: f32 = 2.0 * CH;
/// The share of a Pane's slot a Review opening beside it takes, and how
/// far a drag can take the seam between them (the board's 22–78%).
pub const READER_SHARE: f32 = 0.5;

/// A dragged Pane's drop wash: `DROP_WASH` ground, `DROP_EDGE` edge; its
/// label is a float tag in the float's shell, in the tooltip's pads.
pub const DROP_LABEL_PAD_X: f32 = TOOLTIP_PAD_X;
pub const DROP_LABEL_PAD_Y: f32 = TOOLTIP_PAD_Y;
/// The Pane a live drag picked up, dimmed in its slot until the release.
pub const DRAG_SOURCE_OPACITY: f32 = 0.5;
/// The miniature that rides the pointer while a slot is dragged: see-through
/// enough that the board under it still reads.
pub const DRAG_GHOST_OPACITY: f32 = 0.86;

/// **The home**: what an empty board shows — the steel mark `HOME_MARK_H`
/// tall, then `HOME_MARK_GAP` lower the greeting (`components::greeting`)
/// in `FS_HOME_TITLE` on its own line height, centred.
pub const HOME_MARK_H: f32 = 2.0 * ROW;
pub const HOME_MARK_GAP: f32 = SPACE_4;
/// 24/32 — the home's greeting: the one display size, the scale's top.
pub const FS_HOME_TITLE: f32 = 24.0;
pub const LH_HOME_TITLE: f32 = 32.0;
/// The empty board: one column, the reading column's `TX_COLUMN_MAX_W`
/// wide as the Composer is (narrower in a narrow board, `EMPTY_PAD` clear
/// of its edges), centred and lifted `EMPTY_LIFT` over the middle (the
/// optical centre is above the measured one). Under the
/// greeting, `EMPTY_FACTS_GAP` down, the CLIs found in `FS_META`; then,
/// `EMPTY_SECTION_GAP` apart, the start actions and the recent Threads.
pub const EMPTY_PAD: f32 = SPACE_6;
pub const EMPTY_LIFT: f32 = 64.0;
pub const EMPTY_FACTS_GAP: f32 = SPACE_1_5;
pub const EMPTY_SECTION_GAP: f32 = SPACE_6 + SPACE_2;
/// A home row (an action or a recent Thread): `EMPTY_ROW_H`, radius
/// `R_CONTROL`, `EMPTY_ROW_PAD_X` in; an `EMPTY_ICON_W` column for its
/// icon or state mark, `EMPTY_ROW_GAP` before the label; the key or the
/// project and age at the right in `FS_META` `INK_SUBTLE`. The selected
/// action is `paint::SELECTED`, the rest the row hover.
pub const EMPTY_ROW_H: f32 = ROW + SPACE_3;
pub const EMPTY_ROW_PAD_X: f32 = SPACE_3;
pub const EMPTY_ROW_GAP: f32 = SPACE_3;
pub const EMPTY_ICON_W: f32 = NAV_ICON;
/// The recent heading's foot over its first row.
pub const EMPTY_HEADING_PAD_B: f32 = SPACE_1_5;
/// How many recent Threads the empty board lists (R13).
pub const EMPTY_RECENT_MAX: usize = 4;

/// The provider mark on the empty board's provider rows.
pub const BAR_MARK: f32 = 11.0;

/// **The start hero**, an empty Thread's or a draft's, Codex's home
/// (codex/home.png): the mark `HERO_MARK` square in `HERO_MARK_INK` (its
/// #5d5d5d, x1148–1195, y399–446), `HERO_MARK_GAP` over `What should we
/// build in <project>?` at `FS_HOME_TITLE`, centred over the body.
pub const HERO_MARK: f32 = 48.0;
pub const HERO_MARK_INK: u32 = TEXT_FAINT;
pub const HERO_MARK_GAP: f32 = 28.0;
/// **The env strip** over an empty Thread's or a draft's box, Codex's
/// (codex/home.png): #282828 between the plane #181818 and the box #363636
/// there — here the step between `CARD` and `FIELD` — `ENV_STRIP_H` tall
/// (y894–932), `ENV_STRIP_INSET` in from the box's sides (x818 in a box at
/// x805), a chip's mark `ENV_STRIP_PAD_X` + `FOOTER_CHIP_PAD_X` in (x832),
/// `ENV_MARK` with `ENV_MARK_GAP` to its words, chips `ENV_CHIP_GAP` apart.
pub const ENV_STRIP: u32 = 0x262626;
pub const ENV_STRIP_H: f32 = 38.0;
pub const ENV_STRIP_INSET: f32 = SPACE_3;
pub const ENV_STRIP_PAD_X: f32 = SPACE_2;
pub const ENV_MARK: f32 = 14.0;
pub const ENV_MARK_GAP: f32 = SPACE_1_5;
pub const ENV_CHIP_GAP: f32 = SPACE_3;
// (end WP-C) — append above this line only

// ======================================== WP-D · composer, pickers, usage, draft
// Owner: WP-D (the Composer, its pickers and usage meter, attachments, background chips, the draft.)
// Edit values and append tokens only inside this section.

/// **The Composer is a box in the card** (SPEC §5.5). In Solo it stands
/// under the reading column: at most `TX_COLUMN_MAX_W` wide, centred as
/// the transcript is, and never nearer the card's sides and foot than
/// `COMPOSER_INSET` (the shelf keeps the same column):
/// `paint::FIELD` at `R_FLOAT`, a 1px `FIELD_RING` laid outside it (it takes
/// no layout).
///
/// - **The working line** is not the stack's: it is the transcript's last
///   row while a turn runs (`pane::working_tail`), spaced as the work is.
/// - **The shelf** stands `SHELF_GAP` above the box its files are for:
///   pending files at left, background tasks at right, chips
///   `SHELF_CHIP_H` high at `R_CONTROL` on `paint::RAISED`, `FS_META`.
/// - **The queue strip** (Claude's, when anything is queued): its own card
///   at `R_CARD` on `paint::RAISED`, as wide as the box and
///   `QUEUE_CARD_GAP` over it, a `QUEUE_CARD_H` row per message in the
///   order they go. `N messages queued` at `W_LABEL` leads the first;
///   each message in `INK_SUBTLE` on one cut line, then its own
///   `QUEUED_ACTION` icon buttons: send now, edit, remove. ↑ and ⌫ act
///   on the latest.
/// - **The input** is SF Pro `FS_COMPOSER` on `COMPOSER_ROW_H`, padded
///   `COMPOSER_INPUT_PAD_T` / `COMPOSER_INPUT_PAD_X` / `COMPOSER_INPUT_PAD_B`,
///   growing to `composer::MAX_ROWS` rows. No `❯`: the box says "type
///   here". The placeholder (`PLACEHOLDER_INK`) overlays its first row.
/// - **The caret is a bar** `CARET_W` wide and a row high at the cursor's
///   shaped x, in `CARET`, blinking softly (`MOTION_CARET_BLINK_MS`,
///   `components::caret_blink`) in Solo and the focused board Pane whoever
///   holds the keyboard; an unfocused board Pane draws none (SPEC §5.9).
///   Typing restarts the blink solid.
/// - **The toolbar** (`COMPOSER_TOOLBAR_H`, `COMPOSER_TOOLBAR_PAD_X`): the `+` attach button (`COMPOSER_ATTACH`), a
///   draft's Project and Workspace chips, the permission mode chip, a
///   spacer, the model chip, the effort chip, and the send circle
///   (`COMPOSER_SEND`, `paint::SEND`, `COMPOSER_SEND_GAP` after the chips).
///   A chip is `TOOL_CHIP_H` high at `R_CONTROL`, `TOOL_CHIP_PAD_X` each side,
///   `FS_LABEL`, its parts `TOOL_CHIP_GAP` apart, `paint::RAISED` under the
///   pointer; its menu rests above it.
///
/// A Group Pane's Composer is the compact box (`COMPACT_COMPOSER_*`, SPEC
/// §5.9).
///
/// Any pad or line change here must update `pane::composer_fixed_height` in
/// the same commit.
pub const COMPOSER_ROW_H: f32 = LH_UI;
/// A Markdown table's rows (`rich::style`): it runs the full column, its
/// head `TABLE_HEAD_H` tall at `W_STRONG`, each body row `TABLE_ROW_H`, a
/// `LINE` hairline under every row but the last; a one-line cell's text is
/// centred in its row (Claude: 41 / 37 with the hairline).
pub const TABLE_HEAD_H: f32 = 40.0;
pub const TABLE_ROW_H: f32 = 36.0;
/// The context meter's state steps (`pane::ctx_meter_ink`):
/// `ATTENTION` from `CTX_METER_WARN`, `BLOCKED` from `CTX_METER_FULL`.
pub const CTX_METER_WARN: f32 = 0.60;
pub const CTX_METER_FULL: f32 = 0.85;
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
/// The placeholder stands this far in from the caret's cell, so the bar
/// never touches its first glyph (`↵ to send`).
pub const PLACEHOLDER_GAP: f32 = CARET_W + SPACE_1;
/// One selection colour app-wide: the Composer paints the transcript's
/// native selection wash under its selected runs.
pub const COMPOSER_SELECTION: u32 = TEXT_SELECTION_WASH;
/// An `@`-mention the operator picked: a path, so `LINK_INK` on no
/// ground — the terminal's own colour for a path.
pub const MENTION_INK: u32 = LINK_INK;
/// The panel's Context section (`pane::context_usage`): its stacked bar
/// (4, Claude's), and the gap above the turn's usage under the legend.
pub const USAGE_CARD_GAP: f32 = SPACE_3;
pub const USAGE_CARD_BAR_H: f32 = 4.0;
/// 1px of track between two categories' runs in the context bar
/// (claude/side-panel.png).
pub const USAGE_CARD_BAR_SEAM: f32 = 1.0;
/// Where a usage reading turns from neutral to ATTENTION (80%, rule
/// 2.6.6) — a fraction of the window, not a count — on the status line's
/// account windows and the card alike.
pub const USAGE_TIGHT: f32 = 0.80;
/// The session-controls card: permission modes, MCP servers and background
/// tasks as sections of menu rows, wide enough for a server's name beside
/// its state and two quiet actions (`needs sign-in`, `Sign in`,
/// `Reconnect`).
pub const SESSION_CARD_W: f32 = 416.0;
/// One row of the panel's context legend (Claude's, 24 apart), its colour
/// square, the column its names start on (24 in from the content edge, the
/// square or a list's chevron before it), and the share column the
/// percentages right-align in. Its groups — the window's categories, the
/// deferred ones, the lists — stand `USAGE_LEGEND_GROUP_GAP` apart.
pub const USAGE_LEGEND_ROW_H: f32 = 24.0;
pub const USAGE_SWATCH: f32 = 10.0;
pub const USAGE_LEGEND_TEXT_X: f32 = SPACE_6;
pub const USAGE_SHARE_W: f32 = 44.0;
pub const USAGE_LEGEND_GROUP_GAP: f32 = SPACE_1_5;
/// What fills the context window, one ink per category in the stacked bar
/// and its legend: Claude's saturated category hues (sampled on
/// claude/side-panel.png) — blue messages, orange-red system tools, teal
/// MCP tools, amber system prompt — and its light grey for every other
/// category (skills, memory files, any it names anew), a dark grey for the
/// autocompact buffer and the deferred tools, the track's for free space.
pub const CTX_MESSAGES: u32 = 0x318ce8;
pub const CTX_TOOLS: u32 = 0xe96535;
pub const CTX_MCP: u32 = 0x00a989;
pub const CTX_PROMPT: u32 = 0xd38e2c;
pub const CTX_OTHER: u32 = 0x959595;
pub const CTX_BUFFER: u32 = 0x5e5e5e;
pub const CTX_FREE: u32 = 0x2b2b2b;
pub const CTX_DEFERRED: u32 = CTX_BUFFER;
/// 28px — the air a fitted image keeps from its preview card's edges (the
/// image viewer): a Solo transcript's text inset. Zoomed past the card,
/// the picture pans right to them.
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
/// A pending file (a shelf chip; in Solo inside the box, over the input):
/// the image mark (or a 12px thumbnail) and the name in `LINK_INK`, cut at
/// 32 cells, then the `close` icon (`ATTACH_REMOVE_ICON`) in `INK_SUBTLE` on
/// an `ATTACH_REMOVE`-wide, chip-high slot, a ghost fill under the pointer.
pub const ATTACH_CHIP_H: f32 = SHELF_CHIP_H;
pub const ATTACH_CHIP_MAX_W: f32 = 32.0 * CH;
pub const ATTACH_THUMB: f32 = 12.0;
pub const ATTACH_REMOVE: f32 = SPACE_4;
pub const ATTACH_REMOVE_ICON: f32 = 10.0;
/// An image waiting in the tray (Claude's ≈90px card): the picture
/// covering a `COMPOSER_THUMB` square at `R_CARD` in a 1px `LINE` ring,
/// its caption its tooltip. Under the pointer its `×` stands
/// `THUMB_REMOVE_INSET` in from the top-right corner: a `THUMB_REMOVE`
/// circle of `paint::FLOAT` in a `LINE` ring, the `close` icon at
/// `ATTACH_REMOVE_ICON` in `INK_STRONG`.
pub const COMPOSER_THUMB: f32 = 88.0;
pub const THUMB_REMOVE: f32 = 20.0;
pub const THUMB_REMOVE_INSET: f32 = SPACE_1;
/// In Solo the pending files ride inside the box, a row of chips
/// `COMPOSER_INPUT_PAD_X` in and `COMPOSER_INPUT_PAD_T` under its top (or
/// the queued row), `ATTACH_ROW_GAP` over the text.
pub const ATTACH_ROW_GAP: f32 = SPACE_2;
/// A Composer chip's menu hangs off its chip, its foot 7px over it, its
/// right edge on the chip's (claude/model-menu.png: menu foot y1011, chip
/// top y1018; both right edges x1295).
pub const MODEL_PICKER_GAP: f32 = 7.0;
/// The `+` menu's foot over the Composer box: set 2 to draw Claude's 3-4pt (claude/plus-menu.png;
/// 7 drew about 9 at true size, CP4 #9).
pub const PLUS_MENU_GAP: f32 = 2.0;
/// The Solo Composer's box stands `COMPOSER_OUTSET` past the reading
/// column's text at each side (Claude's: the box frames the column), so at
/// most `COMPOSER_MAX_W` wide and `COMPOSER_INSET` in from the card's sides
/// — the text's own inset less the outset.
pub const COMPOSER_OUTSET: f32 = SPACE_4;
pub const COMPOSER_MAX_W: f32 = TX_COLUMN_MAX_W + 2.0 * COMPOSER_OUTSET;
pub const COMPOSER_INSET: f32 = TX_INSET_SOLO - COMPOSER_OUTSET;
/// A shelf chip: 24 high.
pub const SHELF_CHIP_H: f32 = SPACE_6;
/// The queue strip (Claude's `queued`): a card 8 over the box, a 44 row
/// for each queued message (up to `QUEUE_ROWS_SHOWN`, then it scrolls),
/// its words `QUEUED_PAD_X` in, the count `QUEUED_GAP` before the first
/// message, each row's action buttons (`QUEUED_ACTION`, icons
/// `QUEUED_ICON`) `QUEUED_ACTION_GAP` apart.
pub const QUEUE_CARD_H: f32 = 44.0;
pub const QUEUE_ROWS_SHOWN: usize = 3;
pub const QUEUE_CARD_GAP: f32 = SPACE_2;
pub const QUEUED_PAD_X: f32 = SPACE_4;
pub const QUEUED_GAP: f32 = SPACE_1_5;
pub const QUEUED_ACTION: f32 = SPACE_6;
pub const QUEUED_ACTION_GAP: f32 = SPACE_2;
pub const QUEUED_ICON: f32 = 16.0;
/// The input's padding: 14 above and at the sides, 8 under.
pub const COMPOSER_INPUT_PAD_T: f32 = 14.0;
pub const COMPOSER_INPUT_PAD_X: f32 = 14.0;
pub const COMPOSER_INPUT_PAD_B: f32 = SPACE_2;
/// The toolbar row: 42 high (a 28px chip in 7px of air), 8 in from the
/// box's sides.
pub const COMPOSER_TOOLBAR_H: f32 = 42.0;
pub const COMPOSER_TOOLBAR_PAD_X: f32 = SPACE_2;
/// The hero box's line (a draft's, an empty Thread's: Codex's home
/// composer, codex/home.png y932–986): at least 54 high over its toolbar
/// row, so the box stands two rows tall, 96 in all (Codex's 98).
pub const COMPOSER_HERO_INPUT_H: f32 = 54.0;
/// The `+` attach button (30, off the scale: the mockup's) and its 18px
/// icon (Lucide's `+` fills 14 of 24: Claude's 14pt cross at a 1.5 stroke).
pub const COMPOSER_ATTACH: f32 = 30.0;
pub const COMPOSER_ATTACH_ICON: f32 = 18.0;
/// A toolbar chip: 28 high, 9 in at each side (the mockup's), its parts 6
/// apart; its chevron 11, the mode's shield 14, the provider's logo 13.
pub const TOOL_CHIP_H: f32 = 28.0;
pub const TOOL_CHIP_PAD_X: f32 = 9.0;
pub const TOOL_CHIP_GAP: f32 = SPACE_1_5;
pub const TOOL_CHIP_CHEVRON: f32 = 11.0;
pub const TOOL_CHIP_ICON: f32 = 14.0;
pub const TOOL_CHIP_LOGO: f32 = 13.0;
/// The widest a lead chip (the permission mode, a draft's Project and
/// Workspace) stands in the Solo toolbar. Lead chips never give way to the
/// model chip, whose label cuts instead, so the permission in force stays
/// readable; a longer label (a provider's native mode) cuts here
/// (`pane::composer_toolbar`).
pub const TOOLBAR_LEAD_MAX_W: f32 = 180.0;
/// The send circle (28) and its arrow (14).
pub const COMPOSER_SEND: f32 = 28.0;
pub const COMPOSER_SEND_ICON: f32 = 14.0;
/// The Solo box is one row, as Claude's: the `+` (`COMPOSER_ATTACH`), the
/// line, the send circle, `COMPOSER_ROW_GAP` apart, `COMPOSER_ROW_PAD`
/// inside the box — 44 high at one row, the `+` glyph 14 in and the line
/// 44 in. The line sits `COMPOSER_TEXT_PAD_Y` down so a single row centres
/// on the buttons; as it grows the buttons keep to its foot.
pub const COMPOSER_ROW_PAD: f32 = 7.0;
pub const COMPOSER_ROW_GAP: f32 = 7.0;
pub const COMPOSER_TEXT_PAD_Y: f32 = (COMPOSER_ATTACH - COMPOSER_ROW_H) / 2.0;
/// Under the box, the footer (Claude's): the model and effort as one chip
/// (its words `FOOTER_WORD_GAP` apart), the mode, as plain words in
/// `FS_META` (`FOOTER_CHIP_H` high, `FOOTER_CHIP_PAD_X` each side, a fill
/// only under the pointer, `FOOTER_CHIP_GAP` between chips: 18 from word to
/// word) and the plan-usage ring (`CONTEXT_RING` round, its stroke
/// `CONTEXT_RING_STROKE`: Claude's 14 and 2, on claude/answer-done.png),
/// centred on the `COMPOSER_FOOTER_H` row that is
/// all there is between the box and the card's foot (41, Claude's).
pub const COMPOSER_FOOTER_H: f32 = 41.0;
pub const FOOTER_CHIP_H: f32 = 24.0;
pub const FOOTER_CHIP_PAD_X: f32 = SPACE_1_5;
pub const FOOTER_CHIP_GAP: f32 = SPACE_1_5;
pub const FOOTER_WORD_GAP: f32 = 10.0;
/// The plan-usage popover a click on the ring opens (Claude's, measured on
/// claude/usage-popover.png): `USAGE_POP_W` wide, its lines
/// `USAGE_POP_PAD_Y` in from top and foot and `USAGE_POP_PAD_X` from the
/// sides, its head and each window `USAGE_POP_GAP` apart, each window's
/// bar `USAGE_POP_BAR_H` on `USAGE_POP_TRACK`, `USAGE_POP_BAR_GAP` under its
/// line. Its foot rests `USAGE_POP_OFFSET` above the ring's box, its right
/// edge on the box's.
pub const USAGE_POP_W: f32 = 360.0;
pub const USAGE_POP_PAD_X: f32 = SPACE_4;
pub const USAGE_POP_PAD_Y: f32 = 14.0;
pub const USAGE_POP_GAP: f32 = SPACE_3;
pub const USAGE_POP_BAR_GAP: f32 = 3.0;
pub const USAGE_POP_BAR_H: f32 = 3.0;
pub const USAGE_POP_TRACK: u32 = 0x454545;
/// A window's fill: Claude's accent blue (measured, claude/usage-popover.png).
pub const USAGE_POP_BAR: u32 = 0x237ddb;
pub const USAGE_POP_OFFSET: f32 = SPACE_2;
pub const CONTEXT_RING: f32 = 14.0;
pub const CONTEXT_RING_STROKE: f32 = 2.0;
// (end WP-D) — append above this line only

// ======================================== WP-E · menus, popovers, sheets, notifications
// Owner: WP-E (menus, popovers, Settings, the Project editor, notifications.)
// Edit values and append tokens only inside this section.

/// **One float grammar** (SPEC §5.8, `menu.rs` draws it): every menu,
/// picker, popover, card, toast and sheet is `components::floating_surface`
/// — `paint::FLOAT`, radius `R_FLOAT`, a 1px `paint::FLOAT_EDGE` ring,
/// the one float shadow — the palette's shell at a
/// smaller scale (the shell head names its metrics).
///
/// - **A head** (`menu::head`): one row, the surface's title in
///   `INK_STRONG` at `W_LABEL` at the left, its command or verb in
///   `INK_SUBTLE` at the right (`Model  /model`).
/// - **A section** (`menu::section`): a `FLOAT_SECTION_H` heading in
///   12/500 `INK_SUBTLE`, `FLOAT_SECTION_GAP` above it, a provider's
///   logomark leading in its brand colour.
/// - **A row** (`menu::row`): `FLOAT_ROW_H`, inset `FLOAT_PAD`, radius
///   `R_CONTROL`; a leading mark in its name's ink, the name in `INK_STRONG`, its
///   description `INK_SUBTLE` after it, a key in `INK_SUBTLE` hard right, an
///   accent `✓` on the standing choice. No row is lit at rest; the cursor
///   row is `FLOAT_SEL`, its meta lifted to `INK_STRONG`, a row under the
///   pointer `FLOAT_HOVER`.
/// - **A footer** (`menu::footer`): a `FLOAT_EDGE` rule, then one row
///   of key hints in 12 `INK_SUBTLE` (its `·` too): `↑↓ select · ⏎ open ·
///   esc`.
/// - **A lit token** (`menu::token`): the chosen one of a short ladder
///   (an effort, a two-way setting) on `FLOAT_SEL` in `INK_STRONG` at
///   `W_LABEL` (the primary's white stays the one primary action); the
///   rest `INK_SUBTLE`, `FLOAT_HOVER` under the pointer.
pub const FLOAT_DETAIL_GAP: f32 = MENU_ROW_GAP;
pub const FLOAT_SECTION_GAP: f32 = SPACE_1;
pub const FLOAT_FOOT_H: f32 = FLOAT_ROW_H + 1.0;
/// A ladder's step: 22 high (the bar tab's), radius `R_CONTROL`.
pub const TOKEN_H: f32 = 22.0;
pub const TOKEN_PAD_X: f32 = SPACE_2;
/// The `/` and `@` menus, hung over the Composer: a name column and its
/// description, so wider than Claude's names-only 238
/// (claude/slash-picker.png) but not the palette's 640, which left a
/// third of every row empty. A longer description cuts and keeps its
/// tooltip.
pub const SLASH_MENU_W: f32 = 480.0;
/// The Composer's menus at Claude's widths: the model picker (names only,
/// a model's description in its tooltip), its effort submenu, the
/// permission-mode menu (a name and its one line), and the `+` menu. The
/// submenu opens `SUBMENU_GAP` beside its menu.
pub const MODEL_PICKER_W: f32 = 260.0;
pub const EFFORT_MENU_W: f32 = 318.0;
pub const MODE_MENU_W: f32 = 316.0;
pub const ATTACH_MENU_W: f32 = 228.0;
pub const SUBMENU_GAP: f32 = 2.0;
/// A form field or a sheet's text button: one row in a cell of padding,
/// inside a 1px edge at `R_CONTROL` (a field's the `FIELD_RING` hairline,
/// `CONTROL_FOCUS` while it holds the caret).
pub const FORM_CONTROL_H: f32 = ROW + 2.0 * SPACE_1 + 2.0;
pub const FORM_FIELD_PAD_X: f32 = CH;
pub const FORM_BUTTON_PAD_X: f32 = CH;
/// A tooltip: one meta line (`FS_META` on `LH_META`) `TOOLTIP_PAD_Y`
/// above and below and `TOOLTIP_PAD_X` either side, radius `R_CONTROL`:
/// 26 high, Codex's (codex/model-list.png), its text 8 in, Claude's
/// (claude/slash-picker.png), wrapping past `TOOLTIP_MAX_W`. It is how a
/// truncated label keeps its full value reachable.
pub const TOOLTIP_PAD_X: f32 = SPACE_2;
pub const TOOLTIP_PAD_Y: f32 = SPACE_1;
pub const TOOLTIP_MAX_W: f32 = 320.0;
/// **A toast** hangs top right of the transcript's column, `TOAST_TOP`
/// under what it clears, never over the Composer the operator types in nor
/// the right panel: `TOAST_W` wide (less where the board is narrower), its
/// right edge `TOAST_BOX_RIGHT` in from the board's (the panel's left while
/// it shows), under a Pane's head or tab strip (`PANE_HEAD_H`), so it
/// covers no head's controls (the kit's toast stack stands there too).
/// One row of head (`◆ needs you · <title>`), one body line, then
/// the quick answers: `TOKEN_H` buttons (`Allow 1`, `Deny 3`, `Open ⏎`),
/// `TOAST_QUICK_GAP` over the float's edge.
pub const TOAST_BOX_RIGHT: f32 = SPACE_4;
pub const TOAST_TOP: f32 = SPACE_2;
pub const TOAST_QUICK_GAP: f32 = SPACE_2;
/// **The palette** (command search, ⌘K; SPEC §5.8), measured off the
/// Claude app's own ⌘K search at 1x (2026-11-03): a `PALETTE_W` dialog
/// centred on the board, its head `PALETTE_TOP` under the board's top, over
/// the veil (`VEIL`, the board only), on `paint::FLOAT`, radius `R_FLOAT`,
/// a 1px `FLOAT_EDGE` ring and the deep shadow (`SHADOW_PALETTE_*`). Top
/// to bottom:
///
/// - the input row, `PALETTE_INPUT_H`: the `search` icon
///   (`PALETTE_SEARCH_ICON`) `PALETTE_PAD_X` in — where Claude sets its
///   query — then the query in `FS_UI`, `PALETTE_GAP` between, the status
///   filter's word at the right;
/// - the type tabs, `PALETTE_TABS_H` over the list's hairline (a dialog
///   without tabs rules its input row instead);
/// - the list, `PALETTE_LIST_PAD` in on every side, scrolling past
///   `PALETTE_MAX_H`: section headings `PALETTE_SECTION_H` tall, their
///   13/500 word on the foot less `PALETTE_SECTION_PAD_B`, every heading
///   after the first `PALETTE_SECTION_GAP` lower; rows `PALETTE_ROW_H`,
///   radius `R_CONTROL`, `PALETTE_ROW_PAD_X` in (so a row's mark stands
///   under the input's icon), their parts `PALETTE_GAP` apart — the
///   `PALETTE_MARK_W` mark column, the name, the context at the right,
///   then the `PALETTE_KEYS_W` key column (right-aligned: the row's chord,
///   or on a chordless cursor row the `PALETTE_ENTER_ICON` enter mark), so
///   every context ends on one edge;
/// - the footer, `PALETTE_FOOT_H` under a `FLOAT_EDGE` rule,
///   `PALETTE_PAD_X` in: each hint's word, then its key on a `KEY_CAP`
///   cap, `PALETTE_HINT_GAP` apart.
///
/// The shortcuts sheet takes the same width, input row and veil.
pub const PALETTE_W: f32 = 670.0;
pub const PALETTE_TOP: f32 = 56.0;
/// The dialog is a float (`paint::FLOAT`), so the modal sits a step over
/// the board it covers, never under it; its rows take the float's own
/// steps, `paint::FLOAT_HOVER` under the pointer and `paint::FLOAT_SEL`
/// the cursor. Opaque, as every float is (FL-15).
/// 54: Claude's query sits 27 under the dialog's top.
pub const PALETTE_INPUT_H: f32 = 54.0;
/// 24 — Claude's query, tabs, row marks and footer all start here.
pub const PALETTE_PAD_X: f32 = SPACE_6;
/// 10 — between a row's parts, and the input row's (Claude's mark → name).
pub const PALETTE_GAP: f32 = 10.0;
pub const PALETTE_SEARCH_ICON: f32 = 16.0;
/// 10 — the list's inset on every side; Claude's row fill stands 10 in.
pub const PALETTE_LIST_PAD: f32 = 10.0;
/// 38: the heading, its word's line `PALETTE_SECTION_PAD_B` over its foot,
/// so the first row's fill starts 48 under the hairline (Claude's
/// `Recents`); a later heading stands `PALETTE_SECTION_GAP` taller (41
/// from row to row across Claude's `Actions`).
pub const PALETTE_SECTION_H: f32 = 38.0;
pub const PALETTE_SECTION_PAD_B: f32 = 10.0;
pub const PALETTE_SECTION_GAP: f32 = SPACE_0_5;
pub const PALETTE_ROW_H: f32 = 36.0;
/// 14 — a row's inset: with `PALETTE_LIST_PAD` its mark lands
/// `PALETTE_PAD_X` in, under the input's icon.
pub const PALETTE_ROW_PAD_X: f32 = PALETTE_PAD_X - PALETTE_LIST_PAD;
/// 16 — Claude's row icons.
pub const PALETTE_MARK_W: f32 = 16.0;
pub const PALETTE_MARK_ICON: f32 = 16.0;
pub const PALETTE_ENTER_ICON: f32 = 13.0;
/// The note a list with nothing to show stands in (`No matches`), centred
/// in two rows' height, so an empty result does not collapse the dialog.
pub const PALETTE_EMPTY_H: f32 = 2.0 * PALETTE_ROW_H;
/// The key column: wide enough for a four-glyph chord (`⌃⌥⌘K`) at 12px.
pub const PALETTE_KEYS_W: f32 = SPACE_6 + SPACE_4;
/// 40 — Claude's footer.
pub const PALETTE_FOOT_H: f32 = 40.0;
pub const PALETTE_MAX_H: f32 = 12.0 * PALETTE_ROW_H;
/// ⌘K's type tabs under its input (Claude's search): a row
/// `PALETTE_TABS_H` tall over the list's hairline, its `PALETTE_TAB_H`
/// pills at the row's top from `PALETTE_PAD_X` in, each its word
/// `PALETTE_TAB_PAD_X` in, `PALETTE_TAB_GAP` apart — the chosen one
/// `paint::FLOAT_SEL` in `INK_STRONG`, the rest `INK_SUBTLE`. Claude's:
/// 26 pills, 10 in, 4 apart, 15 of air under them to the hairline.
pub const PALETTE_TABS_H: f32 = 40.0;
pub const PALETTE_TAB_H: f32 = 26.0;
pub const PALETTE_TAB_PAD_X: f32 = 10.0;
pub const PALETTE_TAB_GAP: f32 = SPACE_1;
/// Between the footer's key hints (`Close esc`, `Change type ← →`):
/// Claude's 20.
pub const PALETTE_HINT_GAP: f32 = 20.0;
/// **The find bar** (⌘F): a one-row float `FIND_W` wide, hung
/// `FLOAT_OFFSET` under the top right of the text it searches — the
/// `search` icon (`FIND_ICON`), the query, the count.
pub const FIND_W: f32 = 320.0;
pub const FIND_ICON: f32 = 14.0;
/// **Floats are opaque** (FL-15). gpui blurs no element, and a float as a
/// blurred window of its own is a titled window that takes the keyboard
/// and anchors to nothing (gpui-pre-macos 0.3.3), so floats render
/// in-window (`float::hang`) on their own opaque steps over `FLOAT`.
/// Its ground, edge and inner rules, the cursor row and a row under the
/// pointer are the float paints (`paint::FLOAT*`, FL-14). A quick
/// answer's pill on a float (a toast's buttons) is `paint::FLOAT_HOVER`,
/// stepping up to `paint::FLOAT_SEL` under the pointer.
/// A sheet (Settings, the Project editor, the image preview) is a float at
/// sheet size: its head two rows (the title and the one close control over
/// a `paint::LINE`), its body `MODAL_PAD` in, a footer under a `paint::LINE`.
pub const MODAL_HEAD_H: f32 = 2.0 * ROW;
/// 16 — Claude's confirm dialog sets its title, body and buttons 16 in.
pub const MODAL_PAD: f32 = SPACE_4;
pub const MODAL_GAP: f32 = HALF_ROW;
/// Editors leave an even breathing edge while making room for a scrolling
/// form at short desktop heights.
pub const MODAL_VIEWPORT_FRACTION: f32 = 0.92;
/// A kit-hosted choice menu (`menu::ChoiceMenu`: model, effort, mode,
/// subagent overflow): wide enough for a model name beside its check.
pub const CHOICE_MENU_MIN_W: f32 = 240.0;
pub const CHOICE_MENU_MAX_W: f32 = 320.0;
/// About 48 characters — how much of a long directory a menu row keeps,
/// cut at its head behind `…/` (the tail names the place).
pub const MENU_PATH_TAIL: usize = 48;
// ---------------------------------------------------------- Settings sheet
//
// **The Settings sheet** (`prefs.rs`) is a float at sheet size: the
// float's shell (radius `R_FLOAT`, the `FLOAT_EDGE` ring), SF Pro on the
// space scale, rows and whitespace (no rules between rows), no cards —
// the Claude app's settings.
//
// - **Sidebar** (`SETTINGS_SIDEBAR_W`): the search line on top (the
//   `search` icon, then the field), a `FLOAT_EDGE` rule under it, then one
//   `SETTINGS_NAV_ROW_H` row per page, inset `FLOAT_PAD` like a menu's,
//   radius `R_CONTROL` — `FLOAT_SEL` and `INK_STRONG` on the page shown,
//   `INK_STRONG` on the rest — and `About` at the foot. ↑/↓ step pages while the
//   sheet holds focus.
// - **Content:** the page title in `INK_STRONG` `W_LABEL`, then groups
//   `SETTINGS_GROUP_GAP` apart: a group's label is a `INK_SUBTLE` row (a
//   provider's led by its logomark), then its rows; a list with nothing in
//   it says so in a sentence (`Group::note`).
// - **Row:** the label (`FS_TITLE`, `INK_STRONG`) over its description
//   (`INK_SUBTLE`, wrapping to a second line where it needs one) at the
//   left, `SETTINGS_ROW_PAD_Y` of air above and below; the control at the
//   right, on the label's line. A dependent setting (`Row::nested`) stands
//   `SETTINGS_NEST` in under its parent.
// - **Controls** sit at the row's right: an on/off setting is the one
//   switch (`components::switch`, `SWITCH_*`: a `PRIMARY` track with a dark
//   knob on, a `SWITCH_OFF` track with a `INK_SUBTLE` knob off, the knob
//   sliding over `FADE_QUICK`); a choice of two or three is a segmented
//   control (a `FLOAT_EDGE` ring, the chosen segment `paint::SELECTED`,
//   `SETTINGS_SEGMENT_*`); a longer list is the value and a chevron in a
//   quiet bordered button that opens a menu; an action is the quiet
//   bordered button (`SETTINGS_BUTTON_H`; Settings has no filled button);
//   a size is `‹ 14px ›`. Each is a tab stop with the one focus outline.
// - **About** is a list of facts: the key at the left, the value at the
//   right in `INK_SUBTLE`. A path copies on click.
// - **Search** filters every page at once: matching rows keep their groups,
//   each labelled `Page · Group`; pages with no match read `INK_SUBTLE`.
pub const SETTINGS_W: f32 = 760.0;
pub const SETTINGS_H: f32 = 448.0;
pub const SETTINGS_SIDEBAR_W: f32 = 188.0;
pub const SETTINGS_NAV_ROW_H: f32 = FLOAT_ROW_H;
/// A setting row: its label and description `SETTINGS_ROW_PAD_Y` from
/// the next, `SETTINGS_ROW_GAP` between the words and the control.
pub const SETTINGS_ROW_PAD_X: f32 = SPACE_2;
pub const SETTINGS_ROW_PAD_Y: f32 = SPACE_2;
pub const SETTINGS_ROW_GAP: f32 = SPACE_6;
/// How far a dependent setting stands in under its parent.
pub const SETTINGS_NEST: f32 = SPACE_6;
/// A segment of a short choice, and the quiet bordered button.
pub const SETTINGS_SEGMENT_H: f32 = 22.0;
pub const SETTINGS_SEGMENT_PAD_X: f32 = SPACE_3;
pub const SETTINGS_BUTTON_H: f32 = 26.0;
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
/// The on/off switch (`components::switch`): a `SWITCH_W` × `SWITCH_H`
/// pill, its round knob `SWITCH_INSET` inside it.
pub const SWITCH_W: f32 = 28.0;
pub const SWITCH_H: f32 = SPACE_4;
pub const SWITCH_INSET: f32 = SPACE_0_5;
/// `#4a4a4a` — the off track: a step over the float's cursor row, so it
/// reads on every ground a switch stands on, the float's included.
pub const SWITCH_OFF: Paint = Paint::solid(0x4a4a4a);
// (end WP-E) — append above this line only

// ======================================== WP-F · decisions and subagents
// Owner: WP-F (decision.rs, subagents.rs, the Decision card and keycaps.)
// Edit values and append tokens only inside this section.

/// The Decision (approvals, questions, forms, links — Main's and a
/// Subagent's alike, rule 2.8) is the transcript's tail row, drawn as a box
/// (SPEC §5.9 item 3) across the transcript column, Solo's and a Group
/// Pane's alike: on the plane at `R_CARD`, an inset 1px `CARD_RING` (the
/// transcript cards' one ring), padded `DECISION_PAD`, its sections
/// `DECISION_GAP` apart, its answer row `DECISION_ACTIONS_GAP` under the
/// rest, in SF Pro. The head (`DECISION_HEAD_ICON` `shield` in
/// `ATTENTION`, a question's `?`; the lead 13 at `W_LABEL` `INK_STRONG`;
/// the context at the right in 12 `INK_SUBTLE`), the command well (`SUNKEN`
/// at `R_CONTROL`, Geist Mono 12.5, a `INK_SUBTLE` `$ `), then the options:
/// `DECISION_OPTION_H` rows at `R_CONTROL`, the key in Geist Mono 12 `INK_SUBTLE`,
/// the label in SF Pro 13, the cursor's row `SELECTED` and `INK_STRONG`.
/// No hint line: the keys are on the buttons and the option rows. An
/// approval's box is at most `DECISION_CARD_W` wide, on the column's left
/// (Claude's 480); its head band is 48 (`LH_UI` and `DECISION_HEAD_PAD_Y`
/// above and below), its body inside 16 of padding.
///
/// Where a card still docks over the transcript's foot (the requests
/// overlay) it lies on the plane, `DECISION_PAD_Y` inside.
pub const DECISION_PAD_Y: f32 = HALF_ROW;
/// The card's head band (Claude's): white at 4% over the card's ground,
/// `DECISION_HEAD_PAD_Y` above and below its line.
pub const DECISION_HEAD: u32 = 0xffffff0a;
pub const DECISION_HEAD_PAD_Y: f32 = 14.0;
pub const DECISION_CARD_W: f32 = 480.0;
/// The `Needs your input` pill over a waiting card: 22 high, its outline
/// `ATTENTION` at 45%; `DECISION_PILL_GAP` above the card.
pub const NEEDS_INPUT_H: f32 = 22.0;
pub const NEEDS_INPUT_RING: u32 = 0xe6c47c73;
pub const DECISION_PILL_GAP: f32 = SPACE_2;
/// The approval's answers (Claude's): `Deny` quiet and the split primary
/// `Allow once ⌘⏎` with its `▾` menu (`Always allow <pattern> ⇧⌘⏎`, `Allow
/// and switch to …`), `CONTROL_H` high, `DECISION_ACTION_GAP` apart — inline at the
/// card's right in Solo, stacked full width in a Group Pane. The menu is
/// `DECISION_MENU_W` wide.
pub const DECISION_ACTION_GAP: f32 = SPACE_2;
/// The seam between the split primary's halves: `ON_PRIMARY` at 15%.
pub const PRIMARY_SEAM: u32 = 0x16161626;
pub const DECISION_MENU_W: f32 = 300.0;
pub const DECISION_PAD: f32 = SPACE_4;
pub const DECISION_GAP: f32 = 10.0;
/// 24px — the well to the answer row (Claude's approval).
pub const DECISION_ACTIONS_GAP: f32 = SPACE_6;
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
/// A shell command's ink in the well: `#99ea71`, sampled from
/// claude/approval.png (Claude's command green, brighter than `GREEN`).
pub const SHELL_COMMAND_INK: u32 = 0x99ea71;
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
/// bare input line on the option labels' column and in their face (SF Pro,
/// SPEC §5.9), `LH_UI` high, placeholder `Or type your own answer…` in
/// `INK_SUBTLE`; the digit one past the last option arms it.
pub const QUESTION_OTHER_H: f32 = LH_UI;
/// The primary's `↵`: `ON_PRIMARY` at 70%, so the key reads under its label
/// on the primary fill.
pub const SEND_KEY_INK: u32 = 0x111214b3;
/// A button's key, one cap per key (Claude's `⌘` `↩` after `Allow once`,
/// claude/approval.png x1107–1144): `KEY_CAP` square at `KEY_CAP_R`,
/// `KEY_CAP_GAP` apart, edged in `KEY_CAP_EDGE` on the primary (Claude's
/// #c0c0c0 on its #efefef) and `LINE` elsewhere.
pub const KEY_CAP: f32 = 17.0;
pub const KEY_CAP_GAP: f32 = 3.0;
pub const KEY_CAP_R: f32 = 4.0;
pub const KEY_CAP_EDGE: u32 = 0x11121433;

/// A subagent's head, over its transcript while the Pane shows one (no
/// capture of Claude's drill-in exists: sized as the strip it replaced): one
/// `SUBAGENT_HEAD_H` (38) row at the card's top — under the Group head in a
/// board cell — padded `SUBAGENT_HEAD_PAD_X` (6), closed by a `LINE` the body
/// clips at. `FS_LABEL` SF Pro: the back button (`ICON_BUTTON`), the `Main`
/// crumb `SUBAGENT_CRUMB_H` (28) high at `R_CONTROL` in `INK_SUBTLE`, a
/// `SUBAGENT_CRUMB_ICON` (13) `›` in `TEXT_FAINT`, then the agent's mark in a
/// fixed `SUBAGENT_MARK_W` slot (a nav mark's icon box, the working
/// mark's), its name in `INK_STRONG` and its state word in `INK_SUBTLE`,
/// `SUBAGENT_NAME_GAP` (8) apart.
pub const SUBAGENT_HEAD_H: f32 = 38.0;
pub const SUBAGENT_HEAD_PAD_X: f32 = SPACE_1_5;
pub const SUBAGENT_CRUMB_H: f32 = 28.0;
pub const SUBAGENT_CRUMB_ICON: f32 = 14.0;
pub const SUBAGENT_MARK_W: f32 = NAV_MARK_ICON;
pub const SUBAGENT_NAME_GAP: f32 = SPACE_2;
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
// Chrome is SF Pro; numbers from the terminal world (diffstats, spinners)
// are Geist Mono. Rows are rounded (`R_CONTROL`), wear `paint::RAISED`
// under the pointer (at once, as Claude's rows do) and `paint::SELECTED` with an
// inset `LINE` ring when they stand for what the board shows. Colour is
// state only: the mark (§6), a blocked or failed row's line 2, a Group's
// roll-up word, the needs-you pill.
//
// **Titles truncate against a definite width.** gpui only measures an
// ellipsis against a width it knows on the line's first measure, so every
// truncating box is pinned to what its row leaves it, measured from the
// face's own advances (`nav::Measure`).
//
// **Folding (cmd-B).** The column snaps between `NAV_WIDTH` and the rail
// (`NAV_FOLDED_W`), as Claude's sidebar does: no ride, no fade, no
// per-frame relayout of the board. The rail is one `NAV_RAIL_CELL` per
// open Thread, its mark and its ⌘ digit.

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
/// 4px — between two rows, half above and half below each, so rows step
/// 34, Claude's pitch (geometry.md "Sidebar row height / pitch";
/// sbs3/sidebar.jpg). A row's guides reach across it.
pub const NAV_ROW_GAP: f32 = SPACE_1;
/// 44px — a two-line Thread row: its title, then its branch or reason.
pub const NAV_ROW2_H: f32 = 44.0;
/// 20px — one line of a row (line 1, line 2, a task's extra lines).
pub const NAV_LINE: f32 = ROW;
/// 16px — a view's or a Project's icon (Claude's 16), `NAV_ICON_GAP` before
/// its label, in the label's ink.
pub const NAV_ICON: f32 = 16.0;
pub const NAV_ICON_GAP: f32 = 10.0;
/// 14px — under the views, before the tree.
pub const NAV_VIEWS_GAP: f32 = 14.0;
/// 26px — a section heading (`Projects`, `Groups`, `Recent`): 12/500
/// `INK_SUBTLE`, padding 0 6 0 10, its 22px `+` at the right.
pub const NAV_HEADING_H: f32 = 26.0;
pub const NAV_HEADING_PAD_R: f32 = SPACE_1_5;
pub const NAV_HEADING_BUTTON: f32 = 22.0;
pub const NAV_HEADING_GLYPH: f32 = 14.0;
/// 16px — the sidebar toggle's and the gear's glyph, in an `ICON_BUTTON`;
/// the panel toggle, the sidebar toggle's twin, shares it.
pub const NAV_TOGGLE_GLYPH: f32 = 16.0;
/// 35px — where an Inbox Notice's title starts, past its mark column
/// (`NAV_ROW_PAD_X` + `NAV_ICON` + `NAV_ICON_GAP`, on the view rows'
/// labels); a Notice's mark sits in the 10–24 column before it. Rows under
/// a heading start on its label (`NAV_ROW_PAD_X`, Claude's 18 from the
/// column's edge).
pub const NAV_TITLE_X: f32 = NAV_ROW_PAD_X + NAV_ICON + NAV_ICON_GAP;
/// 18px — one Group level of the tree (§5.1.4): a Group's chevron sits on
/// its row's title column and its title this far past it, where its
/// members' titles start. No guides: the indent says the nesting. Only a
/// row's content moves in; its hover and selected fills keep the full
/// width.
pub const NAV_TREE_INDENT: f32 = NAV_CHEVRON + SPACE_1_5;
/// 8px — between a row's title and what sits at its right.
pub const NAV_WORD_GAP: f32 = SPACE_2;
/// 6px — between two Project sections.
pub const NAV_SECTION_GAP: f32 = SPACE_1_5;
/// A Thread row's marks (§6): 8px solid dots, no halo (Claude's; 6 on a
/// member row), 14px icons (13 on a member row) — the working mark among
/// them.
pub const NAV_DOT: f32 = 8.0;
pub const NAV_DOT_SMALL: f32 = 6.0;
/// The middle of a two-line row's line 1 (its two lines centred in the
/// row): where its mark centres, on the title's own middle (§5.1.5
/// "centred on line 1"; the mockups' marks sit 3–5px under it, on the
/// baseline, which the eye reads as dropped).
pub const NAV_LINE1_MID: f32 = (NAV_ROW2_H - 2.0 * NAV_LINE) / 2.0 + NAV_LINE / 2.0;
/// Where a mark sits, `(x, y)` from its mark column's left and its row's
/// top: centred on line 1 of a two-line row, centred on a one-line row (`_LINE`) and on a shown
/// Group's member (`_SMALL`). A dot centres where an icon
/// does (x 7 on a two-line row).
pub const NAV_DOT_AT: (f32, f32) = (3.0, NAV_LINE1_MID - NAV_DOT / 2.0);
pub const NAV_DOT_AT_SMALL: (f32, f32) = (3.0, (NAV_ROW_H - NAV_DOT_SMALL) / 2.0);
pub const NAV_ICON_AT: (f32, f32) = (0.0, NAV_LINE1_MID - NAV_MARK_ICON / 2.0);
/// An icon on a one-line row, a shown Group's member's included.
pub const NAV_ICON_AT_LINE: (f32, f32) = (0.0, (NAV_ROW_H - NAV_MARK_ICON) / 2.0);
pub const NAV_MARK_ICON: f32 = 14.0;
pub const NAV_MARK_ICON_SMALL: f32 = 13.0;
/// The parked mark's `terminal`, at half ink.
pub const NAV_PARKED_OPACITY: f32 = 0.5;
/// 12px — a Group row's chevron, on its row's title column.
pub const NAV_CHEVRON: f32 = 12.0;
/// 10px — a section heading's caret after its label (`nav::fold_heading`),
/// a step under the Group row's chevron: the heading is the quieter line.
pub const NAV_HEADING_CARET: f32 = 10.0;
/// 6px — between a row's title and the mark (or `⋯`) at its right.
pub const NAV_MARK_GAP: f32 = SPACE_1_5;
/// 5px — an unseen finish's dot at a row's right, in the content ink: a
/// step under the needs-you dot (`NAV_DOT_SMALL`), never orange.
pub const NAV_UNREAD_DOT: f32 = 5.0;
/// 18px — the foot's account disc, its initial at `FS_AVATAR` 500.
pub const NAV_AVATAR: f32 = 18.0;
pub const FS_AVATAR: f32 = 10.0;
/// 12px — a row's PR mark after its `#N` (`nav::pr_mark`): every mark one
/// drawn icon at one size; pending a `NAV_PR_DOT` dot centred in its box.
pub const NAV_PR_MARK: f32 = 12.0;
pub const NAV_PR_DOT: f32 = 6.0;
/// 36px — the foot: `Ferrite 0.5` and the gear, 8px under the tree.
pub const NAV_FOOT_PAD_T: f32 = SPACE_2;
/// The Activity view's row (Codex's): the title on `NAV_LINE` with its
/// mark at the right, then the Project and `NAV_ACTIVITY_PREVIEW_LINES` of
/// the last message on `LH_META`, all `FS_META` `INK_SUBTLE`, padded
/// `NAV_ACTIVITY_PAD_Y` top and bottom.
pub const NAV_ACTIVITY_PAD_Y: f32 = SPACE_1_5;
pub const NAV_ACTIVITY_PREVIEW_LINES: usize = 2;
/// The most characters a folded Project's branch takes before it is cut.
pub const NAV_BRANCH_MAX_CH: usize = 24;
/// 4px — a drop target that takes no layout of its own ("insert above this
/// Group", "append after the last member"): an absolute hit band over the
/// edge of the row it borders.
pub const NAV_DROP_BAND: f32 = SPACE_1;
/// 52px — the column folded by cmd-B: the rail. One `NAV_RAIL_CELL` per
/// open Thread, holding its mark and, under it, its ⌘ digit in 11
/// `INK_SUBTLE`.
pub const NAV_FOLDED_W: f32 = 52.0;
pub const NAV_RAIL_CELL: f32 = 32.0;
pub const NAV_RAIL_KEY_FS: f32 = 12.0;
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
// (end WP-G) — append above this line only

// ======================================== motion
// Owner: the motion kit (motion.rs, and the call sites that ride it).
// Edit values and append tokens only inside this section.
//
// **Motion is a budget, not a garnish.** The numbers are Claude Desktop's,
// read from its source (`docs/research/craft/motion-claude-source.md`, the
// authority wherever it and the recordings disagree): its duration scale
// (`MOTION_DUR_*`, Claude's `--cds-dur-*`) and its curves
// (`MOTION_EASE_*`, `--cds-ease-*`). `motion.rs` maps each entry to the
// Ferrite surface that wears it. The rules every animated surface follows:
//
// - **Menus and popovers snap; dialogs, the palette and toasts fade out.**
//   A menu, a popover, a fold of tool steps, the sidebar (cmd-B) and the
//   panel land on their frame, open and shut, as Claude's do. A sheet, the
//   palette and a toast leave as a face no pointer finds (`motion::float_out`,
//   `toast_out`), their veil fading with them. A board slot is no float:
//   the Review slides in and, a little quicker, out (`MOTION_SLOT_OPEN_MS` /
//   `MOTION_SLOT_CLOSE_MS`), its Pane giving way and taking the room back.
// - **One hover blend for buttons (`MOTION_DUR_FAST_MS`, the quart); list
//   rows and menu rows snap.** A button's ground blends over 60ms and its
//   press squishes it (`MOTION_PRESS_SCALE`); a nav row, a menu row and the
//   sidebar's own buttons take their hover on the frame the pointer
//   arrives. A keyboard change (focus, cmd-] / cmd-D, menu selection, tier
//   changes) lands on its frame.
// - **What arrives on its own animates in**: streamed text by the run
//   (`MOTION_STREAM_FADE_MS`, linear), the operator's sent bubble popping in
//   (`MOTION_BUBBLE_*`), the approval card settling (`MOTION_PROMOTE_*`),
//   the working line's words rolling (`MOTION_MORPH_*`); first paint and
//   scroll-back never animate.
// - **Little moves at once** (theme rule 8). The loops: the working mark
//   (`MOTION_WORKING_MARK_MS`) in place of a working Thread's dot and as the
//   working line's mark, its caption's shimmer (`MOTION_SHIMMER_MS`) on the
//   focused Pane, and the focused Composer's caret blink
//   (`MOTION_CARET_BLINK_MS`). A status dot never pulses or breathes.
// - **Interruptible.** A state the operator can flip back (a hover, a
//   switch, a fold) retargets from where it is, never restarts.
// - **No entrance on first paint.** Only a change the operator watched
//   happen animates; a restored layout or scroll-back arrives in place.
// - **Every animation has a static end state** that says the same thing
//   without motion. Reduced motion (the system flag, `cx.reduce_motion()`)
//   snaps a one-shot to its end, draws no exit, and holds a loop at its
//   start, as Claude's `prefers-reduced-motion` rule does.
// - **A window with nothing animating schedules no frame.** Loops ride one
//   shared, throttled pulse clock (`MOTION_PULSE_TICK_MS`): each declares
//   when its picture next changes, and the clock wakes only the views due
//   then, on its grid; it parks when nothing is declared. The working
//   mark alone moves at every instant: it asks the display for its frames
//   (`motion::frame_loop`), one request a frame for every mark in the
//   window, and only while one can be seen.
// - **A frame redraws only what moves.** Every animation asks for its next
//   frame through the cheapest view that draws it (`motion::frames_through`):
//   a float the Cockpit draws redraws the frame around its cached parts,
//   never every Pane. Nothing animates where its host would rebuild more
//   than the animation shows.

/// Claude's duration scale (`--cds-dur-fast`, `-snap`, `-base`, `-sheet`,
/// `-slow`): every duration below is one of these unless it says why.
pub const MOTION_DUR_FAST_MS: u64 = 60;
pub const MOTION_DUR_SNAP_MS: u64 = 120;
pub const MOTION_DUR_BASE_MS: u64 = 200;
pub const MOTION_DUR_SHEET_MS: u64 = 300;
pub const MOTION_DUR_SLOW_MS: u64 = 450;

/// Zeron's signature entrance curve, CSS `cubic-bezier(0.16, 1, 0.3, 1)`:
/// a Panel preview swapping in, the image viewer's zoom.
pub const MOTION_EASE_OUT_EXPO: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
/// CSS `ease`: the quick fades (an image landing, the Reply pill).
pub const MOTION_EASE: [f32; 4] = [0.25, 0.1, 0.25, 1.0];
/// The CSS keyword `ease-out` (`cubic-bezier(0,0,.58,1)`), which Claude's
/// keyframe shorthands use: the nav's folds.
pub const MOTION_EASE_OUT: [f32; 4] = [0.0, 0.0, 0.58, 1.0];
/// Tailwind's default curve (a bare `transition-*`): the chevrons' turn,
/// the jump button's fade.
pub const MOTION_EASE_STANDARD: [f32; 4] = [0.4, 0.0, 0.2, 1.0];
/// Claude's `--cds-ease-out`, the ease-out quart: button hovers and
/// presses, dialogs, toasts, the usage ring.
pub const MOTION_EASE_OUT_QUART: [f32; 4] = [0.165, 0.84, 0.44, 1.0];
/// Claude's `--cds-ease-snap` (the iOS sheet's curve): it leaves at speed
/// and spends its second half settling. Tooltips, message actions, and a
/// board slot sliding across half the board.
pub const MOTION_EASE_SNAP: [f32; 4] = [0.32, 0.72, 0.0, 1.0];
/// Claude's `--cds-ease-overshoot`: past the end and back (y > 1). The sent
/// bubble's pop, the switch's knob, a button's release.
pub const MOTION_EASE_OVERSHOOT: [f32; 4] = [0.34, 1.3, 0.64, 1.0];
/// The working line's label roll (Claude's `cds-morph`).
pub const MOTION_EASE_MORPH: [f32; 4] = [0.2, 0.0, 0.0, 1.0];
/// The ease-out cubic: the approval card settling.
pub const MOTION_EASE_OUT_CUBIC: [f32; 4] = [0.215, 0.61, 0.355, 1.0];
/// Linear: streamed text (Claude's `delta-fade`).
pub const MOTION_EASE_LINEAR: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// The Composer's box easing to its new height as its text gains or loses
/// a row, on `MOTION_EASE_OUT_QUART`. Claude's `ChatComposer` snaps; its
/// one eased height is the comment card's envelope, over 200ms, which this
/// takes (`motion-claude-source.md`).
pub const MOTION_COMPOSER_GROW_MS: u64 = MOTION_DUR_BASE_MS;

/// `row-in`: 180ms on `MOTION_EASE_OUT_EXPO`, opacity only, nothing moves:
/// a Panel preview or tab body swapping in (`motion::swap_in`).
pub const MOTION_ROW_IN_MS: u64 = 180;
/// Streamed text: each run the provider delivers (a row appended live, or
/// words growing an answer) fades in over 400ms, linear, opacity only
/// (Claude's `delta-fade`). When the turn ends, a run with more than
/// `MOTION_STREAM_LAND_MS` left is sped up to land within it.
pub const MOTION_STREAM_FADE_MS: u64 = 400;
pub const MOTION_STREAM_LAND_MS: u64 = MOTION_DUR_SNAP_MS;
/// The operator's sent bubble pops in over `MOTION_DUR_SLOW_MS` on
/// `MOTION_EASE_OVERSHOOT`: opacity 0 → 1, scale `MOTION_BUBBLE_FROM_SCALE`
/// → 1 from its right edge, rising `MOTION_BUBBLE_RISE` (Claude's
/// `code-user-bubble-enter`).
pub const MOTION_BUBBLE_FADE_MS: u64 = MOTION_DUR_SLOW_MS;
pub const MOTION_BUBBLE_FROM_SCALE: f32 = 0.92;
pub const MOTION_BUBBLE_RISE: f32 = 6.0;
/// The approval card settles in over 220ms on `MOTION_EASE_OUT_CUBIC`, from
/// opacity `MOTION_PROMOTE_FROM_OPACITY`, `MOTION_PROMOTE_DROP` above its
/// place and scale `MOTION_PROMOTE_FROM_SCALE` from its top (Claude's
/// `code-approval-promote`).
pub const MOTION_PROMOTE_MS: u64 = 220;
pub const MOTION_PROMOTE_FROM_OPACITY: f32 = 0.75;
pub const MOTION_PROMOTE_DROP: f32 = 6.0;
pub const MOTION_PROMOTE_FROM_SCALE: f32 = 0.97;
/// The working line's label rolls when its words change: the old face
/// leaves to opacity 0 and `MOTION_MORPH_SHIFT` up as the new one comes in
/// from as far below, each over 180ms on `MOTION_EASE_MORPH`.
pub const MOTION_MORPH_MS: u64 = 180;
pub const MOTION_MORPH_SHIFT: f32 = 3.0;
/// The working line's token count eases to each new value over
/// `MOTION_DUR_SHEET_MS` on the quart, its changing digits rolling up as
/// an odometer's do (`odometer::rolling_count`).
pub const MOTION_COUNT_MS: u64 = MOTION_DUR_SHEET_MS;
/// `fade-quick`: 150ms, opacity only.
pub const MOTION_FADE_QUICK_MS: u64 = 150;
/// The send circle's glyph swap (send ⇄ stop, `fade-quick`): the leaving
/// glyph shrinks to this scale as it fades, the arriving one grows from it
/// (an `svg` transform: the icon swap's scale without its blur).
pub const MOTION_SEND_GLYPH_FROM: f32 = 0.5;
/// `menu-in`: 0ms — a menu snaps open and shut, as Claude's `Menu`,
/// `ContextMenu` and `Popover` do. At 0 the entrance draws its element as
/// it is (`motion::menu_in`); the shift and opacity below are its face
/// should it ever run again.
pub const MOTION_MENU_IN_MS: u64 = 0;
pub const MOTION_MENU_SHIFT: f32 = 2.0;
/// The opacity a menu starts from: it is already legible on its first frame.
pub const MOTION_MENU_FROM_OPACITY: f32 = 0.3;
/// A dialog (the Settings and Project sheets) fades and grows in over
/// `MOTION_DUR_BASE_MS` on the quart, and leaves the same way; its veil
/// fades with it, both ways (Claude's `Dialog`).
pub const MOTION_DIALOG_IN_MS: u64 = MOTION_DUR_BASE_MS;
pub const MOTION_DIALOG_OUT_MS: u64 = MOTION_DUR_BASE_MS;
pub const MOTION_DIALOG_RISE: f32 = 0.0;
/// The palette (and the shortcuts sheet on it) comes in as a dialog does,
/// from its top, and leaves quicker, over `MOTION_DUR_SNAP_MS`; its veil
/// fades over 50ms both ways (Claude's command palette).
pub const MOTION_PALETTE_OUT_MS: u64 = MOTION_DUR_SNAP_MS;
pub const MOTION_PALETTE_VEIL_MS: u64 = 50;
/// The scale a sheet grows from and shrinks to (Claude's 0.98): a CSS
/// `scale` on what it paints, its layout and hit area at rest.
pub const MOTION_FLOAT_FROM_SCALE: f32 = 0.98;
/// A tooltip fades and grows in from `MOTION_FLOAT_FROM_SCALE` over
/// `MOTION_DUR_SNAP_MS` on `MOTION_EASE_SNAP`, after gpui's 300ms delay
/// (Claude's grouped tooltips; the next one within 200ms shows at once).
pub const MOTION_TOOLTIP_IN_MS: u64 = MOTION_DUR_SNAP_MS;
/// A message's actions show under the pointer over `MOTION_DUR_SNAP_MS`
/// after `MOTION_ACTIONS_DELAY_MS`, and go over `MOTION_DUR_FAST_MS` at
/// once, both on `MOTION_EASE_SNAP` (Claude's `--cds-message-actions-opacity`).
pub const MOTION_ACTIONS_DELAY_MS: u64 = 100;
/// A board slot opening beside its owner (the Review beside its Pane)
/// slides in over 240ms on `MOTION_EASE_SNAP`, the owner narrowing with
/// it, and slides out over a shorter 200ms before it is removed.
pub const MOTION_SLOT_OPEN_MS: u64 = 240;
pub const MOTION_SLOT_CLOSE_MS: u64 = 200;
/// The image viewer's zoom (a click between fit and actual size, ⌘= ⌘-
/// ⌘0): 240ms on `MOTION_EASE_OUT_EXPO`, so the picture answers the click
/// at once and settles softly. A pinch or a pan follows the hand instead.
pub const MOTION_IMAGE_ZOOM_MS: u64 = 240;
/// A button's hover blend: `MOTION_DUR_FAST_MS` on the quart (Claude's
/// `Button` fill). Rows and menu rows snap instead.
pub const MOTION_HOVER_FADE_MS: u64 = MOTION_DUR_FAST_MS;
/// A button's press: it squishes to this scale over `MOTION_DUR_FAST_MS`
/// on the quart, and springs back over `MOTION_DUR_SLOW_MS` on
/// `MOTION_EASE_OVERSHOOT` (Claude's `.cds-btn-squish`).
pub const MOTION_PRESS_SCALE: f32 = 0.975;
/// A disclosure chevron turns its quarter over `MOTION_DUR_FAST_MS` on
/// `MOTION_EASE_STANDARD` (Claude's `TurnStatus` caret).
pub const MOTION_CHEVRON_MS: u64 = MOTION_DUR_FAST_MS;
/// A nav fold (a Project, a Group, Pinned) opens and shuts over 150ms on
/// the `ease-out` keyword, height and opacity (Claude's
/// `sidebar-group-expand`).
pub const MOTION_FOLD_MS: u64 = 150;
/// Jump to latest fades in and out over `MOTION_DUR_BASE_MS` on
/// `MOTION_EASE_STANDARD`, answering no pointer while it goes.
pub const MOTION_JUMP_FADE_MS: u64 = MOTION_DUR_BASE_MS;
/// A toast's timing: it comes in over `MOTION_DUR_BASE_MS` on the quart,
/// sliding its own height out from under the edge it hangs on, and leaves
/// over the same, shrinking to `MOTION_TOAST_OUT_SCALE` as it fades (the
/// Needs-you toast, `motion::toast_in` / `toast_out`; the kit's toast stack
/// settles on the same two).
pub const MOTION_TOAST_IN_MS: u64 = MOTION_DUR_BASE_MS;
pub const MOTION_TOAST_OUT_MS: u64 = MOTION_DUR_BASE_MS;
pub const MOTION_TOAST_OUT_SCALE: f32 = 0.85;
/// The usage ring's arc moves to a new reading over `MOTION_DUR_SHEET_MS`
/// and its ink over `MOTION_DUR_BASE_MS`, both on the quart (Claude's
/// `.urm`).
pub const MOTION_USAGE_ARC_MS: u64 = MOTION_DUR_SHEET_MS;
pub const MOTION_USAGE_INK_MS: u64 = MOTION_DUR_BASE_MS;
/// A switch's knob slides over `MOTION_DUR_SNAP_MS` on
/// `MOTION_EASE_OVERSHOOT`; its track snaps (Claude's `Switch`).
pub const MOTION_SWITCH_MS: u64 = MOTION_DUR_SNAP_MS;
/// 1.4s — how long a scrollbar thumb holds after the last scroll frame
/// (C26) before it fades out over `MOTION_SCROLLBAR_FADE_MS`. Idle, no
/// thumb.
pub const MOTION_SCROLLBAR_LINGER_MS: u64 = 1_400;
/// 100ms — a scrollbar thumb's fade out and its widening under the
/// pointer, on `MOTION_EASE_STANDARD`.
pub const MOTION_SCROLLBAR_FADE_MS: u64 = 100;
/// The pulse clock's grid: one ~30fps tick shared by every loop in the
/// window. A loop is drawn on the ticks where its picture changes; one that
/// stops painting drops off after its last declared tick, and the clock
/// parks.
pub const MOTION_PULSE_TICK_MS: u64 = 33;
/// 2.5s — one loop of the working mark (`components::working_mark`):
/// four orbs, one diagonal pair swapping corners every quarter, as Claude
/// Desktop's four-dot glyph does (`docs/research/craft/working-glyph-claude.md`).
/// Drawn at the display's rate, never stepped.
pub const MOTION_WORKING_MARK_MS: u64 = 2_500;
/// 2.4s — one sweep of the shimmer's crest across the working caption,
/// left to right.
pub const MOTION_SHIMMER_MS: u64 = 2_400;
/// The crest's half-width, as a fraction of the caption's width (the
/// prototype's 35%–50%–65% stops over a 300% gradient).
pub const SHIMMER_HALF_WIDTH: f32 = 0.45;
/// 1.0s — the focused Composer's caret: a hard blink, shown for the first
/// half of the period and hidden for the second (500 on, 500 off), as
/// both reference apps blink theirs.
pub const MOTION_CARET_BLINK_MS: u64 = 1_000;
// (end motion) — append above this line only

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face::{be16, covers, table, GEIST_MONO};

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
                _ => INK_SUBTLE,
            };
            assert_eq!(word_ink(word), expected, "{word:?}");
        }
        assert_eq!(word_ink("needs you"), ATTENTION);
        assert_eq!(word_ink("failed"), BLOCKED);
        assert_eq!(word_ink("failing"), BLOCKED);
        assert_eq!(word_ink("done"), INK_SUBTLE);
    }

    /// One waiting agent `needs you`; more `need you`, on every surface.
    #[test]
    fn a_needs_you_count_agrees_with_its_number() {
        assert_eq!(words::needs_you(1), "1 needs you");
        assert_eq!(words::needs_you(2), "2 need you");
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
        ("BLOCK", BLOCK),
        ("FIELD", FIELD),
        ("FIELD_COMPACT", FIELD_COMPACT),
        ("SUNKEN", SUNKEN),
    ];
    /// The raised grounds: a chip, a float, the selected row.
    const RAISED_GROUNDS: &[(&str, u32)] =
        &[("RAISED", RAISED), ("FLOAT", FLOAT), ("SELECTED", SELECTED)];

    #[test]
    fn ink_clears_its_floor_on_every_ground() {
        // The content tier: AA on every ground, raised ones and the
        // sidebar's selected row included.
        let readable = &[("INK_STRONG", INK_STRONG)];
        floor(readable, GROUNDS, 4.5);
        floor(readable, RAISED_GROUNDS, 4.5);
        floor(readable, &[("SIDEBAR_SELECTED", SIDEBAR_SELECTED)], 4.5);
        // The subtle tier is the floor: AA on the plane, the grounds rows
        // sit on and the raised ones.
        floor(&[("INK_SUBTLE", INK_SUBTLE)], GROUNDS, 4.5);
        floor(&[("INK_SUBTLE", INK_SUBTLE)], RAISED_GROUNDS, 4.5);
        // A float's rows: its meta words under the pointer, and lifted to
        // the content tier on the cursor row, where meta falls short.
        floor(
            &[("INK_SUBTLE", INK_SUBTLE)],
            &[("FLOAT_HOVER", FLOAT_HOVER)],
            4.5,
        );
        assert!(contrast(INK_SUBTLE, FLOAT_SEL) < 4.5);
        // Each step reads over the one under it.
        const { assert!(FLOAT_HOVER > FLOAT && FLOAT_SEL > FLOAT_HOVER) };
        assert!(contrast(INK_SUBTLE, CARD) >= 4.5);
        // Comments are the dim ink, under its floor.
        assert_eq!(SYN_COMMENT, INK_SUBTLE);
        // Structure only, never text: a visible glyph on its grounds.
        floor(
            &[("TEXT_FAINT", TEXT_FAINT)],
            &[
                ("CARD", CARD),
                ("FRAME", FRAME),
                ("FLOAT", FLOAT),
                ("BLOCK", BLOCK),
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
            ],
            &[
                ("CARD", CARD),
                ("FRAME", FRAME),
                ("FLOAT", FLOAT),
                ("BLOCK", BLOCK),
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
                &[("SYN_PLAIN", SYN_PLAIN), ("SYN_FUNCTION", SYN_FUNCTION)],
                &[("a diff wash on CARD", over(wash, CARD))],
                4.5,
            );
        }
        // A primary button's label, at rest and under the pointer.
        floor(
            &[("ON_PRIMARY", ON_PRIMARY)],
            &[("PRIMARY", PRIMARY), ("PRIMARY_HOVER", PRIMARY_HOVER)],
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
            &[("INK_STRONG", INK_STRONG)],
            &[("DROP_WASH on CARD", over(DROP_WASH, CARD))],
            4.5,
        );
        floor(
            &[("INK_STRONG", INK_STRONG)],
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

    /// Rule 5: two text tiers (Claude's content and meta), brightest
    /// first, the placeholder and structure under the lot.
    #[test]
    fn the_ink_ladder_is_two_tiers() {
        assert_eq!(
            [INK_STRONG, INK_SUBTLE, PLACEHOLDER_INK],
            [0xefefef, 0x9c9c9c, 0x878787]
        );
        let ladder = [INK_STRONG, INK_SUBTLE, PLACEHOLDER_INK, TEXT_FAINT];
        for pair in ladder.windows(2) {
            assert!(luminance(pair[0]) > luminance(pair[1]), "{pair:06x?}");
        }
    }

    /// Rule 3: the surfaces, darkest first, every one opaque and neutral
    /// grey (no hue anywhere but state), and no accent.
    #[test]
    fn the_surface_ladder_is_ordered_and_grey() {
        let ladder = [
            ("SUNKEN", SUNKEN),
            ("CARD", CARD),
            ("FIELD_COMPACT", FIELD_COMPACT),
            ("RAISED", RAISED),
            ("BLOCK", BLOCK),
            ("SELECTED", SELECTED),
            ("SELECTION_HOVER", SELECTION_HOVER),
            ("FIELD", FIELD),
            ("SEND", SEND),
        ];
        // One plane; the Composer and a float share a grey.
        assert_eq!((FRAME, FLOAT), (CARD, FIELD));
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
            [CARD, BLOCK, FIELD, RAISED, FLOAT, SELECTED, FLOAT_SEL],
            [0x1a1a1a, 0x262626, 0x303030, 0x242424, 0x303030, 0x2a2a2a, 0x3e3e3e]
        );
        assert_eq!((PRIMARY, ON_PRIMARY), (INK_STRONG, 0x161616));
        // Two line greys, and each raised surface's edge a step over it.
        assert_eq!((LINE, CARD_RING), (0x303030, 0x222222));
        assert_eq!(
            [FIELD_RING, FLOAT_EDGE, BLOCK_RING],
            [0x404040, 0x383838, 0x333333]
        );
        for (edge, ground) in [
            (FIELD_RING, FIELD),
            (FLOAT_EDGE, FLOAT),
            (BLOCK_RING, BLOCK),
        ] {
            assert!(
                luminance(edge) > luminance(ground),
                "{edge:06x} on {ground:06x}"
            );
        }
        // The state rings and washes are white at their stated opacities.
        let alpha = |rgba: u32| (rgba & 0xff) as f32 / 255.;
        for (line, opacity) in [(FOCUS_RING, 0.22), (DROP_EDGE, 0.22), (DROP_WASH, 0.06)] {
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
            ("INLINE_CODE_INK", INLINE_CODE_INK),
            ("PRIMARY", PRIMARY),
        ] {
            assert_eq!(ink, INK_STRONG, "{name} is the strongest ink");
        }
        // A link is the one ink that is not state (rule 6); a mention is a
        // path, so a link.
        assert_ne!(LINK_INK, INK_STRONG, "links wear the link blue");
        assert_eq!(MENTION_INK, LINK_INK);
        assert!(mode_is_manual("default"), "Manual reads amber");
        for quiet in ["acceptEdits", "plan", "bypassPermissions", "auto", "never"] {
            assert!(!mode_is_manual(quiet), "{quiet} wears the plain meta ink");
        }
        assert_eq!(IDLE, INK_SUBTLE, "done and idle wear no hue");
    }

    /// Rule 4: every region is an opaque grey, the hairlines white
    /// overlays, and the frame sits a quiet step over the card.
    #[test]
    fn the_paints_are_opaque_greys() {
        let paints = [
            (paint::CARD, CARD),
            (paint::FRAME, FRAME),
            (paint::SIDEBAR, SIDEBAR),
            (paint::SIDEBAR_HOVER, SIDEBAR_HOVER),
            (paint::SIDEBAR_SELECTED, SIDEBAR_SELECTED),
            (paint::BLOCK, BLOCK),
            (paint::FIELD, FIELD),
            (paint::RAISED, RAISED),
            (paint::SELECTED, SELECTED),
            (paint::SELECTION_HOVER, SELECTION_HOVER),
            (paint::FLOAT, FLOAT),
            (paint::FLOAT_EDGE, FLOAT_EDGE),
            (paint::FLOAT_HOVER, FLOAT_HOVER),
            (paint::FLOAT_SEL, FLOAT_SEL),
            (paint::LINE, LINE),
            (paint::CARD_RING, CARD_RING),
            (paint::FIELD_RING, FIELD_RING),
            (paint::BLOCK_RING, BLOCK_RING),
        ];
        for (paint, grey) in paints {
            assert_eq!(paint.rgba(), solid(grey));
            assert!(paint.is_opaque());
        }
        assert_eq!(paint::WINDOW, paint::CARD);
        assert_eq!(paint::PANEL, paint::FRAME);
        assert_eq!(FRAME, CARD, "one plane");
        // The sidebar sits a step over the plane; its rows step up from it.
        assert!(CARD < SIDEBAR && SIDEBAR < SIDEBAR_HOVER && SIDEBAR_HOVER < SIDEBAR_SELECTED);
        assert_eq!(paint::PRESS, paint::SELECTION_HOVER);
        // Hover is one equal step on every ground (rule 3).
        for ground in [CARD, FRAME, BLOCK, FLOAT] {
            let step = (over(HOVER_WASH, ground) & 0xff) - (ground & 0xff);
            assert!((3..=4).contains(&step), "{ground:06x} steps {step}");
        }
        // A hovered row stays under the selection.
        assert_eq!(over(HOVER_WASH, CARD), 0x1e1e1e);
        assert!(luminance(over(HOVER_WASH, CARD)) < luminance(SELECTED));
        // A paint converts the way `rgba()` does, never the way `rgb()` would.
        let fill: gpui::Fill = paint::CARD.into();
        assert_eq!(fill, gpui::Fill::from(gpui::rgba(paint::CARD.0)));
        let hsla: gpui::Hsla = paint::RAISED.into();
        assert_eq!(hsla, paint::RAISED.hsla());
    }

    /// Rule 2: radii by role (SPEC §3.6), and only floats cast.
    #[test]
    fn radii_follow_their_roles_and_only_floats_cast() {
        assert_eq!([R_CONTROL, R_CARD, R_FLOAT], [6.0, 10.0, 12.0]);
        assert_eq!(R_TIGHT, 2.0);
        const { assert!(SHADOW_FLOAT_Y > 0.0 && SHADOW_FLOAT_BLUR > 0.0) };
    }

    /// Rule 1: every type role on a whole-pixel line, a 1.5× reading line
    /// at every reading size, headings at the body size, a code cell of
    /// 0.6em, and every chrome size on the type scale.
    #[test]
    fn every_type_role_has_a_whole_pixel_line_box() {
        use ferrite_core::settings::ReadingSize;
        assert_eq!((FS_UI, LH_UI), (14.0, 20.0), "chrome is 14 on 20");
        for (size, line) in [(FS_PROSE, LH_PROSE), (FS_UI, LH_UI)] {
            assert_eq!(line, line.round());
            assert!(line >= size * 1.35, "{size}px on a {line}px line box");
        }
        for reading in ReadingSize::STEPS.map(ReadingSize::nearest) {
            let (size, line) = (answer_text_size(reading), answer_line_height(reading));
            assert_eq!(line, line.round());
            assert!(line >= size * 1.45, "{reading:?}: {size}/{line}");
            assert!(line <= size * 1.55, "{reading:?}: {size}/{line}");
        }
        // At Standard: Solo and a Group Pane 14/21, on the scale.
        assert_eq!(answer_line_height(ReadingSize::nearest(14)), 21.0);
        let group = tx_size(ReadingSize::STANDARD, false);
        assert_eq!((group, prose_line_height(group)), (14.0, 21.0));
        let solo = tx_size(ReadingSize::STANDARD, true);
        assert_eq!((solo, prose_line_height(solo)), (14.0, 21.0));
        for size in [group, solo, group - TX_BLOCK_STEP, solo - TX_BLOCK_STEP] {
            assert!(TYPE_SCALE.contains(&size), "{size} is off the scale");
        }
        assert_eq!(answer_text_size(ReadingSize::STANDARD), FS_PROSE);
        assert_eq!(answer_line_height(ReadingSize::STANDARD), LH_PROSE);
        for level in 1..=6 {
            assert_eq!(heading_scale(level), 1.0, "H{level} is the body size");
        }
        assert_eq!(prose_line_height(FS_PROSE), LH_PROSE);
        // The chrome's roles (SPEC §3.5).
        assert_eq!(
            [FS_TITLE, FS_LABEL, FS_META, FS_BADGE, FS_COMPOSER],
            [14.0, 13.0, 13.0, 12.0, 14.0]
        );
        for size in [
            FS_UI,
            FS_PROSE,
            FS_TITLE,
            FS_LABEL,
            FS_META,
            FS_BADGE,
            FS_COMPOSER,
            FS_PANEL_DIFF,
            FS_CHROME_CODE,
            FS_HOME_TITLE,
            NAV_RAIL_KEY_FS,
        ] {
            assert!(TYPE_SCALE.contains(&size), "{size} is off the scale");
        }
        assert_eq!(
            (W_BODY, W_LABEL, W_STRONG),
            (FontWeight::NORMAL, FontWeight::MEDIUM, FontWeight::SEMIBOLD)
        );
        for line in [LH_META, LH_PANEL_DIFF] {
            assert_eq!(line, line.round());
        }
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(near(CODE_CELL, 7.8) && near(CH, CODE_CELL));
        assert!(near(GLYPH_GUTTER, 2.0 * CH));
        assert_eq!((ROW, HALF_ROW), (20.0, 10.0));
        assert_eq!(TITLEBAR_H, 44.0);
        assert_eq!((NAV_WIDTH, BOARD_PAD, GRID_GAP), (256.0, 8.0, 8.0));
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
            assert_eq!(tokens.button_primary_hover.color, solid(PRIMARY_HOVER));
            assert_eq!(tokens.button_primary_foreground.color, solid(ON_PRIMARY));
            assert_eq!(tokens.primary.color, solid(PRIMARY));
            assert_eq!(tokens.accent.color, paint::FLOAT_SEL.hsla());
            assert_eq!(tokens.popover.color, paint::FLOAT.hsla());
            assert_eq!(tokens.muted.color, solid(RAISED));
            assert_eq!(tokens.tab_active.color, solid(SELECTED));
            assert_eq!(tokens.ring.color, solid(CONTROL_FOCUS));
            assert_eq!(tokens.input.color, solid(INPUT_EDGE));
            assert_eq!(tokens.border.color, paint::LINE.hsla());
            assert_eq!(tokens.selection.color, alpha(TEXT_SELECTION_WASH));
            assert_eq!(tokens.caret.color, solid(INK_STRONG));
            assert_eq!(tokens.link.color, solid(LINK_INK));
            assert_eq!(tokens.sidebar.color, solid(SIDEBAR));
            assert_eq!(tokens.drop_target.color, alpha(DROP_WASH));
            // And the colours the non-token paths read.
            assert_eq!(theme.primary, solid(PRIMARY));
            assert_eq!(theme.info, solid(INK_STRONG), "no accent hue");
            assert_eq!(theme.muted, solid(RAISED));
            assert_eq!(theme.background, solid(CARD));
            assert!(!theme.shadow, "floats wear Ferrite's own shadow");
            assert_eq!(theme.radius, gpui::px(R_CONTROL));
            assert_eq!(theme.radius_lg, gpui::px(R_FLOAT));
            assert_eq!(theme.font_size, gpui::px(FS_UI));
            assert_eq!(theme.mono_font_size, gpui::px(FS_UI));
            assert_eq!(theme.font_family.as_ref(), FONT_UI);
            assert_eq!(theme.mono_font_family.as_ref(), "Geist Mono");
        });
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

    /// Rule 1: two faces. The system's UI font sets what is read, SF Pro
    /// Mono (bundled, its five weights and nothing else) sets code.
    #[test]
    fn faces_are_the_system_ui_font_and_the_bundled_mono() {
        assert_eq!((FONT_UI, FONT_CODE), (".SystemUIFont", "Geist Mono"));
        assert!(crate::FONTS.iter().all(|face| family(face) == FONT_CODE));
        assert_eq!(crate::FONTS.len(), 5, "regular, italic and three weights");
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
    }

    /// Rule 10 over this crate's render code (`face`); the Pane, the
    /// Cockpit and the app each check their own.
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        crate::face::assert_render_code_draws_only_covered_glyphs(&[
            ("menu.rs", include_str!("menu.rs")),
            ("components.rs", include_str!("components.rs")),
            ("keymap.rs", include_str!("keymap.rs")),
            ("scrollbar.rs", include_str!("scrollbar.rs")),
            ("lib.rs", include_str!("lib.rs")),
        ]);
    }
}
