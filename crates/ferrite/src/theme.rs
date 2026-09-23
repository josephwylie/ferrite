//! Ferrite's visual system: every colour, face, size and metric, named once.
//! This module doc is where the design rules live; there is no other design
//! document. Render code imports from here and holds no colour or metric
//! literal of its own; core stays colour-blind.
//!
//! **Terminal grammar, application craft.** Ferrite keeps the provider CLIs'
//! vocabulary (`❯` prompts, tool bullets, result elbows, a monospace voice for
//! everything structural) and renders it cleanly. No chat bubbles, no avatars,
//! and no raw TUI dump where every line has one weight and colour decorates.
//!
//! The rules every render site follows:
//!
//! 1. **One hue family.** Neutrals carry a trace of chroma at hue 258, the app
//!    icon's hue. The accent (`ACCENT` and its family) is that hue with more
//!    chroma, and it marks the prompt `❯`, the caret, links, focus, selection
//!    and primary actions. Nothing else is blue.
//! 2. **Colour is state.** `RUNNING`, `ATTENTION` and `BLOCKED` mark status
//!    only. A failure colours the word that says so, never the whole row.
//!    Green never means "finished". Provider logomarks are monochrome except
//!    inside the provider/model picker rows.
//! 3. **Opaque faces, alpha edges.** Planes and hover/fill faces are opaque
//!    `rgb()` values (a hover must never be tinted by what lies under it, see
//!    `pointer.rs`). Hairlines, washes, rings over content and veils are alpha
//!    `rgba()` values.
//! 4. **An ordered elevation ladder.** `GROUND` (window, nav, board) <
//!    `PANE` < `RAISED` (Composer, code, cards, menus) < `RAISED_2` (keycaps,
//!    chips on a raised block) < `FILL` (selected) < `FILL_HOVER`. `HOVER` is
//!    the hover face on `GROUND`/`PANE` only; on `RAISED` the hover face is
//!    `FILL`, because `HOVER` would be invisible there. Floating surfaces are
//!    `RAISED` + a `HAIRLINE_STRONG` edge + `R_BLOCK` + a float shadow; in-flow
//!    blocks and planes cast no shadow; a modal adds `VEIL`.
//! 5. **An ink ladder with floors**, brightest first: `TEXT_STRONG` (titles,
//!    prompts, headings), `TEXT` (agent prose, the brightest body copy),
//!    `TEXT_2` (secondary copy), `TEXT_MUTED` (metadata; the floor for readable
//!    text, at least 4.5:1 on every plane), `TEXT_FAINT` (structure only:
//!    glyphs, rules, separators; at least 3:1 on `GROUND`/`PANE`).
//!    **`TEXT_FAINT` is never text.** A row spends at most two text inks, one
//!    glyph ink and one state colour.
//! 6. **Two faces.** `FONT_MONO` (Geist Mono) is the structural voice: chrome,
//!    nav, Pane heads, prompts, tool activity, the Composer, code, menus.
//!    `FONT_PROSE` (Geist) is for what an operator reads at length: agent
//!    prose, Decision questions, option descriptions. Mono uses weights 400
//!    and 500 only, 500 for a surface's single title; 600 is prose only
//!    (headings, `**strong**`, the Decision question); 700 is unused.
//! 7. **Pixel line heights.** Every text role is a (size, line height) pair,
//!    and fixed row heights are `const` expressions of those pairs, never
//!    hand-summed literals.
//! 8. **A space scale:** 2 · 4 · 6 · 8 · 12 · 16 · 20 · 24 · 32 (`SPACE_*`,
//!    named in gpui's 4px units). A metric off the scale says why in its doc.
//!    More space above a heading or a new turn than below it.
//! 9. **Radii say role, not size:** Pane 10 · block 8 · control 6 · chip 4 ·
//!    tight 3. A nested radius is the outer radius less its inset.
//! 10. **Glyph coverage.** A glyph outside the bundled Geist Mono cmap is
//!     never text on any surface; it is an SVG in a glyph box. `CHROME_GLYPHS`
//!     lists the non-ASCII glyphs text may use, and a test checks them against
//!     the bundled face.
//!
//! **Layout of this file.** Everything down to `init_components` is the frozen
//! shared head: values more than one work package reads, and the kit mapping.
//! Below it, one section per work package (`WP-A` … `WP-G`), each opened by a
//! banner and closed by an `(end WP-x)` line. A package edits values and
//! appends tokens only inside its own section; a token two packages need is a
//! request to the integrator, who adds it to the head.
//!
//! The contrast floors, the ladder order, the kit-token mapping and the
//! derived row heights are asserted in `theme::tests`. Dark only: operators
//! work long sessions beside dark editors and terminals.

use gpui::FontWeight;

// ---------------------------------------------------------------- planes

/// `#0d0e11` — the window's own ground: the nav, the Cockpit field, the
/// gutters between Panes. The darkest plane.
pub const GROUND: u32 = 0x0d0e11;
/// `#131518` — a Pane's plane, one step above the ground, so a Pane reads as
/// a sheet laid on the field without needing a heavy edge.
pub const PANE: u32 = 0x131518;
/// The nav column: the ground itself. Navigation is the field the Panes sit
/// on, not a slab of its own.
pub const NAV: u32 = GROUND;
/// The Pane header: the Pane's own plane. The header is chrome by its type and
/// its hairline, not by a band.
pub const PANE_HEAD: u32 = PANE;
/// `#1a1d21` — raised in-flow blocks: the Composer, code blocks, cards, and
/// every floating surface's ground.
pub const RAISED: u32 = 0x1a1d21;
/// The floating menu ground. Same value as `RAISED`, its own name so a retune
/// can split them without a rename.
pub const MENU: u32 = RAISED;
/// `#1d2024` — a row's or control's hover face on `GROUND` or `PANE` only.
/// On `RAISED` the hover face is `FILL`.
pub const HOVER: u32 = 0x1d2024;
/// `#21252a` — one step above `RAISED`: keycaps, chips on a raised block.
pub const RAISED_2: u32 = 0x21252a;
/// `#24282e` — the selected fill (the focused Thread's row, an active tab),
/// and the hover face of anything on `RAISED` (menu rows included).
pub const FILL: u32 = 0x24282e;
/// `#2b2f36` — a filled row under the pointer.
pub const FILL_HOVER: u32 = 0x2b2f36;
/// The pressed shade: one step past `FILL`.
pub const PRESSED: u32 = FILL_HOVER;

// ------------------------------------------------------------------ edges

/// `#ffffff14` (8%) — the one rule weight: a Pane's resting edge, the rule
/// under the Pane head, table rows, separators.
pub const HAIRLINE: u32 = 0xffffff14;
/// `#ffffff24` (14%) — the stronger rule: floating edges (menu, popover,
/// tooltip, toast), the Composer's resting edge, a blockquote's rule.
pub const HAIRLINE_STRONG: u32 = 0xffffff24;
/// The hairline under the Pane header.
pub const PANE_HEAD_EDGE: u32 = HAIRLINE;
/// The Composer's resting edge.
pub const COMPOSER_EDGE: u32 = HAIRLINE_STRONG;
/// Rules between transcript table rows.
pub const TABLE_RULE: u32 = HAIRLINE;
/// `#32363c` — the 1px rail that indents a Group's member Threads in the nav.
#[allow(dead_code)]
pub const GROUP_RAIL: u32 = 0x32363c;
/// `#696f78` — a resting checkbox, radio or switch boundary: solid and at
/// least 3:1 on `PANE` and `RAISED`, so an unchecked control never vanishes.
pub const INPUT_EDGE: u32 = 0x696f78;
/// `#2f3339` / `#43484f` — the scrollbar thumb, at rest and under the pointer.
pub const SCROLLBAR: u32 = 0x2f3339;
pub const SCROLLBAR_HOVER: u32 = 0x43484f;
/// `#ffffff1a` — an unlit tasks-meter segment and the usage lines' tracks.
#[allow(dead_code)]
pub const METER_OFF: u32 = 0xffffff1a;
/// `#000000a6` — the veil behind a modal sheet.
pub const VEIL: u32 = 0x000000a6;
/// Fully transparent — a Pane's edge is always in layout; only its colour
/// changes, so nothing reflows when a Decision or a blocker arrives.
pub const TRANSPARENT: u32 = 0x00000000;

// -------------------------------------------------------------------- ink

/// `#eef0f3` — titles, the operator's own prompt text, headings, bold runs,
/// a Decision's question.
pub const TEXT_STRONG: u32 = 0xeef0f3;
/// `#d9dce2` — agent prose, the brightest *body* copy on screen, and a
/// Thread row's title.
pub const TEXT: u32 = 0xd9dce2;
/// `#abb0b8` — secondary copy: tool summaries, descriptions, blockquotes.
pub const TEXT_2: u32 = 0xabb0b8;
/// `#8b919b` — metadata: checkout lines, tool arguments, durations, hints,
/// timestamps, placeholders. The floor for any text that must be read: at
/// least 4.5:1 on every plane, `FILL` included.
pub const TEXT_MUTED: u32 = 0x8b919b;
/// `#616670` — structure, never words: the `·` seam, disclosure glyphs,
/// elbows, rules. At least 3:1 on `GROUND` and `PANE`.
pub const TEXT_FAINT: u32 = 0x616670;

// ----------------------------------------------------------------- accent

/// `#8daedf` — steel blue from the app icon (hue 258): the prompt `❯`, links,
/// the active indicator, a selected check, a drop target.
pub const ACCENT: u32 = 0x8daedf;
/// `#b3cbed` — the icon's light stop: link hover, accent on `FILL` or on a
/// selection.
pub const ACCENT_HI: u32 = 0xb3cbed;
/// `#4368a0` — the accent as a fill: the primary button (white ink 5.6:1).
pub const ACCENT_STRONG: u32 = 0x4368a0;
/// A primary button under the pointer and held down.
pub const PRIMARY_HOVER: u32 = 0x5075af;
pub const PRIMARY_ACTIVE: u32 = 0x395b90;
/// Ink on an `ACCENT_STRONG` fill.
pub const ON_ACCENT: u32 = 0xffffff;
/// `#6381b0` — **the** keyboard-focus ink: the focused Pane's ring, the kit's
/// `ring`, every focus outline. At least 3:1 on `GROUND` and `PANE`.
pub const FOCUS_RING: u32 = 0x6381b0;
/// `#8daedf66` — the accent as an outline that is not focus: a link's
/// underline, a selected choice's edge, a drop target's edge.
pub const ACCENT_EDGE: u32 = 0x8daedf66;
/// `#8daedf24` (14%) — the accent as a ground: inline code, a selected accent
/// row, the slot a dragged Pane would take.
pub const ACCENT_WASH: u32 = 0x8daedf24;
/// `#8daedf40` (25%) — native text selection, painted over glyphs.
pub const TEXT_SELECTION_WASH: u32 = 0x8daedf40;
/// `#2a384f` — the Composer's opaque selection quad.
pub const SELECTION: u32 = 0x2a384f;
/// The caret.
pub const CARET: u32 = ACCENT;

// -------------------------------------------------------- state + signals

/// `#7cc49a` — live work: the running status dot, a running signal line, the
/// pass chip, diff `+`.
pub const RUNNING: u32 = 0x7cc49a;
/// Running as a ground: an added hunk row, the pass chip.
pub const RUNNING_WASH: u32 = 0x7cc49a1f;
/// The halo that breathes behind a working Thread's dot in the nav.
pub const RUNNING_HALO: u32 = 0x7cc49a59;
/// `#e2b86b` — a Decision: the status dot, the signal line, the Pane's edge,
/// the Decision card's mark.
pub const ATTENTION: u32 = 0xe2b86b;
/// A Decision card's ground.
pub const ATTENTION_WASH: u32 = 0xe2b86b14;
/// A Decision card's 1px inset ring. An inset ring takes no layout.
pub const ATTENTION_EDGE: u32 = 0xe2b86b59;
/// `#e8877c` — blocked or failed: the status dot, the signal line, the
/// Pane's edge, diff `−`, the word "failed".
pub const BLOCKED: u32 = 0xe8877c;
/// Blocked as a ground: a removed hunk row.
pub const BLOCKED_WASH: u32 = 0xe8877c1f;
/// The idle/parked status dot: the muted ink in a dot role.
pub const IDLE: u32 = TEXT_MUTED;

/// Brand marks, not UI colour: only the provider/model picker rows wear them.
#[allow(dead_code)]
pub const PROVIDER_CODEX: u32 = 0x10a37f;
#[allow(dead_code)]
pub const PROVIDER_CLAUDE: u32 = 0xd97757;

// ------------------------------------------------------- transcript colour

/// Syntax sits in the accent family (hue 258) so code never reads as state.
/// Keywords.
pub const SYN_KEYWORD: u32 = 0xa2c0eb;
/// Function names.
pub const SYN_FUNCTION: u32 = 0xc8d5e8;
/// Type names, at hue 240 so they separate from keywords.
pub const SYN_TYPE: u32 = 0xaecce2;
/// String literals: a green quieter than `RUNNING`.
pub const SYN_STRING: u32 = 0x9fcfa8;
/// Number literals, at hue 65 so a number never reads as a Decision.
pub const SYN_NUMBER: u32 = 0xe0b48b;
/// Comments are read, not decoration: at least 4.5:1 on `RAISED`.
pub const SYN_COMMENT: u32 = 0x818790;
/// Punctuation.
pub const SYN_PUNCT: u32 = TEXT_MUTED;
/// Everything the highlighter leaves unclassed.
pub const SYN_PLAIN: u32 = TEXT;
/// A link's ink and its underline.
pub const LINK_INK: u32 = ACCENT;
/// The wash over the slot a dragged Pane would take.
pub const DROP_WASH: u32 = ACCENT_WASH;

// ---------------------------------------------------------------- shadows

/// `--shadow-float` layer 1: `0 10px 28px -10px rgba(0,0,0,0.62)`.
pub const SHADOW_FAR: u32 = 0x0000009e;
#[allow(dead_code)]
pub const SHADOW_FAR_Y: f32 = 10.0;
#[allow(dead_code)]
pub const SHADOW_FAR_BLUR: f32 = 28.0;
#[allow(dead_code)]
pub const SHADOW_FAR_SPREAD: f32 = -10.0;
/// `--shadow-float` layer 2: `0 2px 6px rgba(0,0,0,0.3)`.
pub const SHADOW_NEAR: u32 = 0x0000004d;
#[allow(dead_code)]
pub const SHADOW_NEAR_Y: f32 = 2.0;
#[allow(dead_code)]
pub const SHADOW_NEAR_BLUR: f32 = 6.0;

// ------------------------------------------------------------------- type

/// 14px — agent prose (Geist), the size an operator reads at length; also the
/// Decision question and option descriptions. Paired with `LH_PROSE`.
pub const FS_PROSE: f32 = 14.0;
/// 12.5px — the mono UI size: prompts, tool rows, the Composer, menu rows,
/// nav and Pane titles, code. Paired with `LH_UI` (single-line rows) or
/// `LH_CODE` (multi-line mono blocks).
pub const FS_UI: f32 = 12.5;
/// 12.5px — secondary prose (Geist): option labels and descriptions, notes.
/// Prose is never smaller. Paired with `LH_PROSE_SM`.
pub const FS_PROSE_SM: f32 = 12.5;
/// 11.5px — metadata: checkout lines, durations, hints, keycaps, chips,
/// timestamps. Paired with `LH_META`.
pub const FS_SM: f32 = 11.5;

/// 22px — prose.
pub const LH_PROSE: f32 = 22.0;
/// 18px — secondary prose.
pub const LH_PROSE_SM: f32 = 18.0;
/// 20px — single-line mono rows.
pub const LH_UI: f32 = 20.0;
/// 18px — multi-line mono blocks: code, diffs, tool output.
pub const LH_CODE: f32 = 18.0;
/// 16px — metadata at `FS_SM`.
pub const LH_META: f32 = 16.0;
/// 16px — the stacked two-line rows (a nav row's title over its meta).
pub const LH_TIGHT: f32 = 16.0;

/// Weights: 400 body; 500 a surface's single title and labels; 600 prose
/// headings, `**strong**` and the Decision question. 700 is not used.
pub const W_BODY: FontWeight = FontWeight::NORMAL;
pub const W_LABEL: FontWeight = FontWeight::MEDIUM;
pub const W_STRONG: FontWeight = FontWeight::SEMIBOLD;

/// Solo's optional reading scale affects prose, not execution or chrome. It
/// applies only in Solo and fullscreen; a Group is always Standard.
pub fn answer_text_size(size: ferrite_core::settings::SoloReadingSize) -> f32 {
    use ferrite_core::settings::SoloReadingSize;
    match size {
        SoloReadingSize::Standard => FS_PROSE,
        SoloReadingSize::Comfortable => 16.,
        SoloReadingSize::Large => 18.,
    }
}

/// The pixel line height paired with each reading size: 14/22, 16/24, 18/28.
pub fn answer_line_height(size: ferrite_core::settings::SoloReadingSize) -> f32 {
    use ferrite_core::settings::SoloReadingSize;
    match size {
        SoloReadingSize::Standard => LH_PROSE,
        SoloReadingSize::Comfortable => 24.,
        SoloReadingSize::Large => 28.,
    }
}

/// Headings are ratios of the answer size: H1 18/14, H2 16/14, H3–H6 1.0
/// (set apart by weight and ink, not by a half pixel). At Standard that is
/// 18 · 16 · 14.
pub fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 18. / 14.,
        2 => 16. / 14.,
        _ => 1.,
    }
}

/// A prose size's pixel line height: `round(size × 22/14)`.
pub fn prose_line_height(size: f32) -> f32 {
    (size * LH_PROSE / FS_PROSE).round()
}

/// 0.6em — Geist Mono's advance width (600/1000 em), the pitch a
/// per-character cell must be laid out on so it cannot round up to a whole
/// pixel.
#[allow(dead_code)]
pub const MONO_ADVANCE: f32 = 0.6;
/// 7.5px — one mono column at `FS_UI`.
pub const MONO_CELL: f32 = FS_UI * MONO_ADVANCE;

/// The non-ASCII glyphs mono text may use: every one is in the bundled Geist
/// Mono cmap (asserted by `theme::tests`). Anything else — `❯ ⎿ ∴ ✻ ✓ ✗ ☐`
/// and friends — is an SVG in a glyph box, never text.
pub const CHROME_GLYPHS: &[char] = &[
    '↳', '±', '↑', '↓', '⇥', '↵', '⌫', '•', '●', '…', '→', '·', '−', '│', '└', '─', '›',
];

/// 720px — the reading column's maximum width, gutter included. Wide Panes
/// centre the column; narrow Panes use their full width. The Composer, the
/// working line and a Decision share the column's edges.
pub const READING_MAX_W: f32 = 720.0;

// ------------------------------------------------------------------ space

/// The space scale, in gpui's 4px-unit names (`SPACE_2` = `.p_2()` = 8px).
pub const SPACE_0_5: f32 = 2.0;
pub const SPACE_1: f32 = 4.0;
pub const SPACE_1_5: f32 = 6.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_5: f32 = 20.0;
pub const SPACE_6: f32 = 24.0;
pub const SPACE_8: f32 = 32.0;

// ------------------------------------------------------------------ radii

/// 10px — a Pane.
pub const R_PANE: f32 = 10.0;
/// 8px — blocks: the Composer, code, cards, menus, popovers, toasts. The
/// kit's `radius_lg`.
pub const R_BLOCK: f32 = 8.0;
/// 6px — controls: buttons, nav rows, pickers. The kit's `radius`.
pub const R_CONTROL: f32 = 6.0;
/// 4px — chips, keycaps, inline code, and a menu's rows (`R_BLOCK` less the
/// menu's 4px inset).
pub const R_CHIP: f32 = 4.0;
/// 3px — meter segments and other tiny marks only.
pub const R_TIGHT: f32 = 3.0;

// --------------------------------------------------------- shell and board

/// 286px — the navigation column. The collapsed rail is 77px on macOS so
/// it owns the same horizontal reserve as the native traffic-light group;
/// the project title therefore starts beyond the window controls. Other
/// platforms keep the conventional compact 56px rail.
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input.
#[allow(dead_code)]
pub const NAV_WIDTH: f32 = 286.0;
#[allow(dead_code)]
pub const NAV_RAIL_WIDTH: f32 = if cfg!(target_os = "macos") {
    TRAFFIC_RESERVE
} else {
    56.0
};
/// 42px — the window-chrome band at the top of the nav (traffic lights and
/// the collapse button). **The Cockpit has no band of any kind above it:**
/// the Pane grid starts at y = 0.
#[allow(dead_code)]
pub const WIN_CHROME_H: f32 = 42.0;
/// 77px — the horizontal room the window-chrome band reserves before the
/// collapse button: the traffic lights plus the prototype's 8px flex gap
/// and 4px button margin. Measured from the prototype (button left edge
/// x = 77). On macOS the *host* lights occupy it; nothing else may be drawn
/// there, and nothing interactive may sit in the band's top 28px or AppKit's
/// native drag region stops working.
#[allow(dead_code)]
pub const TRAFFIC_RESERVE: f32 = 77.0;
/// Where the host traffic-light group's close button sits: 13px in from the
/// window's left edge, vertically centred for a 14px button in the 42px band.
pub const TRAFFIC_X: f32 = 13.0;
pub const TRAFFIC_Y: f32 = 14.0;
/// The Pane board: 8px gap on both axes, 10px padding on all four sides.
/// (The prototype's own render reserves 58px at the bottom for its
/// mode-switcher; that is prototype-only chrome and its `data-view="window"`
/// rule restores 10px. Port 10px.)
pub const GRID_GAP: f32 = 8.0;
pub const GRID_PAD: f32 = 10.0;
/// Where the board starts: under the titlebar band, then the same 10px it
/// keeps on its other three sides. Flush to the band, a Pane's top-right
/// corner sits directly beneath the caption buttons, and their hover face
/// — edge-to-edge by design — reads as lying over the Pane.
pub const BOARD_TOP: f32 = WIN_CHROME_H + GRID_PAD;
/// 320px — a toast's width: a Thread's name, a detail line, room for the
/// kit's icon and close button.
pub const TOAST_W: f32 = 320.0;

// -------------------------------------------- pane body and the row column

/// 16px — the inline padding every Pane strip shares.
pub const PANE_PAD_X: f32 = SPACE_4;
/// The Pane body's padding: 16px top, so the first line never kisses the
/// head rule, and 32px bottom, the room the working line overlays.
pub const BODY_PAD_T: f32 = SPACE_4;
#[allow(dead_code)]
pub const BODY_PAD_B: f32 = SPACE_8;
/// 12px — the glyph box every transcript and Composer row hangs its mark in
/// (`❯`, a tool dot, the answer mark, an elbow).
pub const GLYPH_BOX: f32 = 12.0;
/// 8px — from the glyph box to the row's text.
pub const GUTTER_GAP: f32 = 8.0;
/// 20px — **C1**, the text column of every transcript and Composer row:
/// `GLYPH_BOX + GUTTER_GAP`. An elbow result sits at C2 = C1 + `ELBOW_INDENT`.
pub const GUTTER_W: f32 = GLYPH_BOX + GUTTER_GAP;
/// C2 − C1: an elbow row indents by one gutter.
#[allow(dead_code)]
pub const ELBOW_INDENT: f32 = GUTTER_W;
/// 13px — a raised box's content inset (1px edge + 12px padding). Transcript
/// rows sit the same distance inside the reading column, so the transcript `❯`
/// and the Composer `❯` share one axis.
#[allow(dead_code)]
pub const BOX_INSET_X: f32 = 13.0;
/// 8px — between a row's glyph column and its text (tool rows, the working
/// line, the turn diff, controls beside a label).
#[allow(dead_code)]
pub const EVENT_GAP: f32 = 8.0;
/// 10px — between transcript blocks and Markdown siblings.
pub const BLOCK_GAP: f32 = 10.0;
/// 1px — the focused Pane's ring (`FOCUS_RING` ink), lying exactly on the
/// Pane's own border box: focus changes colour and nothing else. It is an
/// absolutely positioned overlay inside a non-clipping wrapper, since a ring
/// drawn inside the shell's `overflow_hidden()` would be clipped.
pub const FOCUS_RING_W: f32 = 1.0;

// ------------------------------------------------ shared controls and rows

/// 28px — an icon button, a rail item and the Project filter trigger.
#[allow(dead_code)]
pub const ICON_BUTTON: f32 = 28.0;
/// 16px — an icon button's glyph, centred in `ICON_BUTTON`.
#[allow(dead_code)]
pub const ICON_BUTTON_GLYPH: f32 = 16.0;
/// 12px — the chevron beside a picker or a disclosure.
#[allow(dead_code)]
pub const ICON_CHEVRON: f32 = 12.0;
/// 28px — a text button in pane and nav chrome, decisions and footers, with
/// 12px inline padding. Sheet controls are `FORM_CONTROL_H`.
pub const CONTROL_H: f32 = 28.0;
pub const CONTROL_PAD_X: f32 = SPACE_3;
/// A keycap: 18px high (it fits inside a 20px UI row), 5px inline padding.
pub const KBD_H: f32 = 18.0;
pub const KBD_PAD_X: f32 = 5.0;
/// 5px — between keycaps, and between a hint's key and its verb.
#[allow(dead_code)]
pub const KEYS_GAP: f32 = 5.0;
/// A chip: 20px high, 6px inline and 1px block padding — the mode chip, the
/// pass chip, the changed strip's file chips, background tasks.
pub const CHIP_H: f32 = 20.0;
#[allow(dead_code)]
pub const CHIP_PAD_X: f32 = 6.0;
#[allow(dead_code)]
pub const CHIP_PAD_Y: f32 = 1.0;
/// 4px — a floating menu's inset around its rows (`FLOAT_PAD`).
pub const MENU_PAD: f32 = 4.0;
/// 28px — one menu row: `LH_UI` plus 4px above and below.
pub const MENU_ROW_H: f32 = 28.0;
/// A floating surface's inset around its rows (the same 4px as `MENU_PAD`),
/// and the gap it keeps from the control that opened it.
pub const FLOAT_PAD: f32 = MENU_PAD;
#[allow(dead_code)]
pub const FLOAT_OFFSET: f32 = 6.0;
/// The context menu's width, and any floating list's height cap before it
/// scrolls.
#[allow(dead_code)]
pub const MENU_W: f32 = 256.0;
#[allow(dead_code)]
pub const MENU_MAX_H: f32 = 420.0;
/// A menu row's inline padding and the gap between its label and trailing
/// parts; its radius nests inside the surface (`R_BLOCK` − `FLOAT_PAD`).
pub const MENU_ROW_PAD_X: f32 = 8.0;
pub const MENU_ROW_GAP: f32 = SPACE_3;
pub const R_MENU_ROW: f32 = R_CHIP;
/// A menu section title row, and the space either side of a separator.
pub const MENU_SECTION_H: f32 = 24.0;
pub const MENU_SEP_Y: f32 = 4.0;
/// An aligned name column (slash commands): clamped between these.
pub const MENU_NAME_MIN_W: f32 = 96.0;
pub const MENU_NAME_MAX_W: f32 = 220.0;
/// A nav or list row's padding — 8px inline, 6px block — and no gap between
/// its stacked lines: their pixel line boxes already carry the air.
#[allow(dead_code)]
pub const ROW_PAD_X: f32 = 8.0;
#[allow(dead_code)]
pub const ROW_PAD_Y: f32 = 6.0;
#[allow(dead_code)]
pub const ROW_GAP: f32 = 0.0;
/// 44px — a Thread row: its padding around a title line over a meta line,
/// 6 + 16 + 0 + 16 + 6. Derived from the type, never summed by hand.
#[allow(dead_code)]
pub const THREAD_ROW_H: f32 = 2.0 * ROW_PAD_Y + LH_TIGHT + ROW_GAP + LH_META;
/// A Group parent row: the same two lines, so the same 44px.
#[allow(dead_code)]
pub const GROUP_ROW_H: f32 = THREAD_ROW_H;
/// The folder and branch marks on the Project and checkout lines (12px), and
/// the 5px gap to their labels.
#[allow(dead_code)]
pub const ROW_ICON: f32 = 12.0;
#[allow(dead_code)]
pub const ROW_ICON_GAP: f32 = 5.0;
/// 12px — the provider logomark in a picker row and the Composer's chip.
#[allow(dead_code)]
pub const PROVIDER_MARK_SM: f32 = 12.0;
/// 24px — an L2 cell's header row; 10px its padding.
pub const CELL_HEADER_H: f32 = 24.0;
pub const CELL_PAD: f32 = 10.0;
/// 24px — one queued prompt's row pitch in the Composer's queue viewport.
pub const QUEUE_ROW_H: f32 = 24.0;

// ------------------------------------------------------- status and motion

/// 6px — the status dot: the Pane head, nav rows, the wall.
pub const STATUS_DOT: f32 = 6.0;
/// 4px — how far the working halo reaches past its dot on every side, so
/// the breathing circle is 14px across.
pub const STATUS_HALO_INSET: f32 = 4.0;
/// 1.4s — one full breath of a working Thread's dot. Slow enough to read
/// as breathing rather than blinking.
pub const STATUS_PULSE_MS: u64 = 1_400;
/// The dimmest the halo goes: never all the way out, so the dot keeps a
/// ring at the bottom of the breath instead of flickering off.
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
/// 120ms ease-out — every hover/press transition the prototype declares.
/// gpui 0.2.2 refines styles without interpolation; recorded, not applied.
#[allow(dead_code)]
pub const TRANSITION_MS: u64 = 120;

// ------------------------------------------------------------------ faces

/// The mono face, **bundled**: Geist Mono. It is the structural voice —
/// chrome, nav, Pane heads, prompts, tool activity, the Composer, code.
///
/// gpui has no variation-axis support, so `main.rs` registers static
/// instances (Regular, Italic, Medium, SemiBold, Bold). They share the
/// typographic family name (name ID 16), and CoreText/DirectWrite resolve the
/// face from `.font_weight(..)`. **Never reach a weight by family name**:
/// `.font_family("Geist Mono Medium")` silently resolves to the fallback face.
pub const FONT_MONO: &str = "Geist Mono";

/// The prose face, bundled: Geist. Agent prose, Decision questions, option
/// descriptions — what an operator reads at length.
pub const FONT_PROSE: &str = "Geist";

/// The chrome face. Chrome is mono (operator decision), so this is
/// `FONT_MONO` under its role name; the kit's `font_family` is set from it.
pub const FONT_UI: &str = FONT_MONO;

/// Install Longbridge once per app, then map its semantic theme to Ferrite's
/// existing tokens. Constructors also call this for standalone test windows.
/// The window mounts the toolkit Root for Settings search input and focus.
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
    theme.font_family = FONT_UI.into();
    theme.font_size = px(FS_UI);
    theme.mono_font_family = FONT_MONO.into();
    theme.mono_font_size = px(FS_UI);
    theme.radius = px(R_CONTROL);
    theme.radius_lg = px(R_BLOCK);
    // Floating surfaces carry `HAIRLINE_STRONG` edges; the kit's own shadow
    // stays off so every float wears one recipe.
    theme.shadow = false;
    theme.motion.spring_move = gpui::base::Spring::new(std::time::Duration::from_millis(120))
        .with_damping(1.0)
        .with_epsilon(0.1);
    theme.background = rgb(PANE).into();
    theme.foreground = rgb(TEXT).into();
    theme.border = rgba(HAIRLINE_STRONG).into();
    // Kit menu and completion rows sit on `RAISED`, where `HOVER` would be
    // invisible: their hover face is `FILL`.
    theme.accent = rgb(FILL).into();
    theme.accent_foreground = rgb(TEXT_STRONG).into();
    theme.secondary = rgb(RAISED).into();
    theme.secondary_hover = rgb(FILL).into();
    theme.secondary_active = rgb(FILL_HOVER).into();
    theme.secondary_foreground = rgb(TEXT_2).into();
    theme.primary = rgb(ACCENT_STRONG).into();
    theme.primary_hover = rgb(PRIMARY_HOVER).into();
    theme.primary_active = rgb(PRIMARY_ACTIVE).into();
    theme.primary_foreground = rgb(ON_ACCENT).into();
    // `Button::primary` reads its own fields, not `primary`.
    theme.button_primary = rgb(ACCENT_STRONG).into();
    theme.button_primary_hover = rgb(PRIMARY_HOVER).into();
    theme.button_primary_active = rgb(PRIMARY_ACTIVE).into();
    theme.button_primary_foreground = rgb(ON_ACCENT).into();
    theme.muted = rgb(RAISED).into();
    theme.muted_foreground = rgb(TEXT_MUTED).into();
    theme.popover = rgb(MENU).into();
    theme.popover_foreground = rgb(TEXT).into();
    theme.ring = rgb(FOCUS_RING).into();
    theme.caret = rgb(ACCENT).into();
    theme.selection = rgba(TEXT_SELECTION_WASH).into();
    theme.link = rgb(ACCENT).into();
    theme.link_hover = rgb(ACCENT_HI).into();
    theme.link_active = rgb(ACCENT).into();
    // Native checkbox/radio indicators use `input` for their resting edge.
    theme.input = rgb(INPUT_EDGE).into();
    theme.switch = rgb(RAISED_2).into();
    theme.switch_thumb = rgb(TEXT_STRONG).into();
    theme.overlay = rgba(VEIL).into();
    theme.danger = rgb(BLOCKED).into();
    theme.warning = rgb(ATTENTION).into();
    theme.success = rgb(RUNNING).into();
    theme.info = rgb(ACCENT).into();
    theme.list_hover = rgb(HOVER).into();
    theme.list_active = rgb(FILL).into();
    theme.table_head = rgb(PANE).into();
    theme.table_head_foreground = rgb(TEXT_MUTED).into();
    theme.drag_border = rgb(ACCENT).into();
    theme.drop_target = rgba(ACCENT_WASH).into();
    theme.sidebar = rgb(GROUND).into();
    theme.sidebar_foreground = rgb(TEXT_2).into();
    theme.sidebar_accent = rgb(HOVER).into();
    theme.sidebar_accent_foreground = rgb(TEXT_STRONG).into();
    theme.sidebar_border = rgba(TRANSPARENT).into();
    // No track: only the thumb is ever ink, and it lightens rather than
    // darkens when the pointer takes hold of it.
    theme.scrollbar = rgba(TRANSPARENT).into();
    theme.scrollbar_thumb = rgb(SCROLLBAR).into();
    theme.scrollbar_thumb_hover = rgb(SCROLLBAR_HOVER).into();
    // `Theme::change` resolved the kit's pre-computed tokens from its default
    // palette, and nothing recomputes them: widgets that read `tokens.*`
    // (Button::primary, menu rows, tooltips, checkboxes) would paint the
    // kit's neutrals. Rebuild them from the colours above.
    theme.tokens = gpui::component::ThemeTokens::from(&theme.colors);
    // Toasts stack at the board's top-right corner, inside its own
    // padding, so they cover a Pane's head and never the nav or the bell
    // that lists them. Five at once is a wall's worth; the bell holds the
    // rest.
    theme.notification.placement = gpui::Anchor::TopRight;
    theme.notification.margins = gpui::base::Edges {
        top: px(BOARD_TOP),
        right: px(GRID_PAD),
        bottom: px(GRID_PAD),
        left: px(GRID_PAD),
    };
    theme.notification.width = px(TOAST_W);
    theme.notification.max_items = 5;
}

// ======================================== end of the frozen shared head

// ======================================== WP-A · transcript rows and grammar
// Owner: WP-A (transcript.rs, pane/text.rs, the transcript rows in pane.rs, ferrite-core transcript strings.)
// Edit values and append tokens only inside this section.

/// An added diff line's code: `RUNNING` lifted a step to read on its wash.
pub const DIFF_ADDED_INK: u32 = 0xa7d9b8;
/// A removed diff line's code: `BLOCKED` lifted the same step.
pub const DIFF_REMOVED_INK: u32 = 0xefa89f;
/// 9px — the tool/event rows' glyph column today, and 8px (`EVENT_GAP`) to
/// the verb beside it; their sum, 17px (`INDENT`), is the inset a result
/// line and a hunk share. WP-A replaces it with the shared `GUTTER_W` (C1).
pub const EVENT_GUTTER_W: f32 = 9.0;
pub const INDENT: f32 = 17.0;
/// 15px — an answer's Ferrite mark. It draws wider than the `GUTTER_W`
/// gutter it hangs in and out of the flow, so its overhang lands in the
/// answer row's own `ANSWER_GAP` rather than moving the prose.
pub const ANSWER_MARK: f32 = 15.0;
/// The offset that centres that mark on the first prose line box at the
/// Standard reading size (`LH_PROSE`). Other sizes add half their line box's
/// difference from `LH_PROSE`.
pub const ANSWER_MARK_TOP: f32 = (LH_PROSE - ANSWER_MARK) / 2.0;
/// 14px — the answer row's gutter-to-prose gap, wider than the `EVENT_GAP`
/// the tool rows use: an answer's prose is indented off the mark rather than
/// held on the tool rows' text edge, and the gap clears the mark's overhang.
pub const ANSWER_GAP: f32 = 14.0;
/// Structured answers retain a passage boundary without isolating every update.
pub const ANSWER_PAD_Y: f32 = 8.0;
/// A single prose paragraph sits closer to the work it introduces.
pub const COMMENTARY_PAD_Y: f32 = 4.0;
/// A tool row's vertical padding. The prototype's 3px each side put 43px
/// between consecutive calls; a run of shell commands reads as a list only
/// when they sit as close as Claude Code's own `●`/`⎿` pairs do.
pub const EVENT_PAD_Y: f32 = 1.0;
/// The result line's padding: hugging its call above, a hair under.
pub const RESULT_PAD_T: f32 = 0.0;
pub const RESULT_PAD_B: f32 = 1.0;
/// An invisible hit area, not a drawn thing: the tool-disclosure target.
pub const TOOL_DISCLOSURE_HIT: f32 = 20.0;
/// A 16px list indent, with a 4px disc 15px left of the text.
pub const UL_INDENT: f32 = 16.0;
#[allow(dead_code)]
pub const BULLET_D: f32 = 4.0;
#[allow(dead_code)]
pub const BULLET_OFFSET: f32 = 15.0;
/// 5px — the operator's prompt block's block padding: the ground the line
/// stands on (`--raised`, or the provider's wash on a Thread), so a prompt
/// reads apart from an answer.
pub const PROMPT_PAD_Y: f32 = 5.0;
/// A hunk row: 8px inline padding, a 24px right-aligned number column, a
/// 7px sign column, 10px between columns. A hunk sits 4px below the event
/// and 10px above what follows.
#[allow(dead_code)]
pub const HUNK_PAD_X: f32 = 8.0;
pub const DIFF_NUM_W: f32 = 24.0;
#[allow(dead_code)]
pub const DIFF_SIGN_W: f32 = 7.0;
#[allow(dead_code)]
pub const DIFF_GAP: f32 = 10.0;
#[allow(dead_code)]
pub const HUNK_MARGIN_T: f32 = 4.0;
/// How many rows one hunk card draws before it stops and says how many it
/// did not. An edit's patch is a handful of lines; a written file's is
/// however long the file is, and a card that redrew a 900-line file would
/// be the transcript rather than a note in it.
pub const HUNK_MAX_ROWS: usize = 24;
// (end WP-A) — append above this line only

// ======================================== WP-B · markdown, prose, scrollbars
// Owner: WP-B (rich.rs, scrollbar.rs, attachments::inline_file, the Markdown vendor knobs.)
// Edit values and append tokens only inside this section.

/// **Markdown.** Agent prose is Geist in `TEXT` at the reading size
/// (`answer_text_size`, set by the answer row). Blocks sit `PROSE_GAP` apart;
/// a heading takes more space above (`PROSE_GAP + HEADING_SPACE_ABOVE` = 20)
/// than below (`HEADING_SPACE_BELOW` = 8). H1–H3 are `W_STRONG`
/// `TEXT_STRONG`, H4–H6 `W_LABEL` `TEXT_2`, never italic or underlined, each
/// on its own pixel line (`prose_line_height`). Tables are horizontal
/// hairlines only; a quote is a 2px `TEXT_FAINT` rule and `TEXT_2`, not
/// italic; list markers are `TEXT_MUTED` in the vendor's measured column.
/// Code is a `RAISED` block; inline code is mono on an `ACCENT_WASH` chip;
/// links are `ACCENT` over an `ACCENT_EDGE` underline.
///
/// 12px — between Markdown blocks (`SPACE_3`).
pub const PROSE_GAP: f32 = SPACE_3;
/// 8px — added above a heading that follows a sibling, on top of
/// `PROSE_GAP`, so a heading opens a section rather than closing one.
pub const HEADING_SPACE_ABOVE: f32 = SPACE_2;
/// 8px — below a heading, in place of `PROSE_GAP`.
pub const HEADING_SPACE_BELOW: f32 = SPACE_2;
/// Inline code's ink on its chip (the Markdown path paints the chip; the
/// plain-text fallback carries the ink alone).
pub const INLINE_CODE_INK: u32 = TEXT_STRONG;
/// The inline-code chip reaches 2px past its glyphs and stays 2px inside the
/// line box top and bottom (18px tall on a 22px line). Painted, never laid
/// out.
pub const INLINE_CODE_OVERHANG: f32 = SPACE_0_5;
pub const INLINE_CODE_INSET_Y: f32 = SPACE_0_5;
/// A quote's rule and its text inset.
pub const QUOTE_RULE_W: f32 = 2.0;
pub const QUOTE_PAD_L: f32 = SPACE_3;
/// 6px — a table cell's block padding (its inline padding is the vendor's
/// 8px, which its column measurement assumes).
pub const TABLE_CELL_PAD_Y: f32 = SPACE_1_5;
/// The rule under a table's header row: one step stronger than the rows'.
pub const TABLE_HEAD_RULE: u32 = HAIRLINE_STRONG;
/// 4px — a horizontal rule's own margin inside its block, so it sits 16px
/// from its neighbours.
pub const RULE_MARGIN_Y: f32 = SPACE_1;
/// A fenced code block: 12px inline, 10px block padding (Zeron's code body;
/// 10 is off the scale so the 24px header and an 18px line land on even
/// pixels).
pub const CODE_PAD_X: f32 = SPACE_3;
pub const CODE_PAD_Y: f32 = 10.0;
/// The code header row: language label, html `Preview`, `Copy`/`Copied`.
pub const CODE_HEADER_H: f32 = 24.;
/// Code actions keep a stable target when Copy becomes Copied.
pub const CODE_ACTION_H: f32 = 24.;
pub const CODE_ACTION_MIN_W: f32 = 56.;
pub const CODE_ACTION_PAD_X: f32 = SPACE_2;
/// The html preview dialog: the reading column's width, and a height cap
/// before its body scrolls.
pub const HTML_PREVIEW_MAX_H: f32 = 520.0;
/// An inline file chip: `CHIP_H` tall so it fits a 22px prose line without
/// moving it; 6px inline padding; a 12px file mark (or a 14px thumbnail) 6px
/// from the name; clamped between 64 and 280px wide.
pub const INLINE_FILE_H: f32 = CHIP_H;
pub const INLINE_FILE_PAD_X: f32 = SPACE_1_5;
pub const INLINE_FILE_GAP: f32 = SPACE_1_5;
pub const INLINE_FILE_ICON: f32 = 12.0;
pub const INLINE_FILE_THUMB: f32 = 14.0;
pub const INLINE_FILE_MIN_W: f32 = 64.0;
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

/// The seam's grab band under the pointer: a faint lift over the gutter.
pub const SEAM_HOVER: u32 = 0xffffff14;
/// A row's opacity while it is being dragged.
#[allow(dead_code)]
pub const DRAGGING_OPACITY: f32 = 0.4;
/// The Windows caption buttons (`titlebar.rs`), which exist only where the
/// app draws its own titlebar. 46px is the width Windows gives each of its
/// own — the snap-layout flyout aligns to it, so a narrower button would
/// hang the flyout off-centre — and they run the band's full 42px height,
/// flush to the window's top-right corner.
#[allow(dead_code)]
pub const CAPTION_W: f32 = 46.0;
/// 10px — the caption mark inside that button. Window chrome is smaller
/// than UI: `ICON_BUTTON_GLYPH` at 16px would read as an app control.
#[allow(dead_code)]
pub const CAPTION_GLYPH: f32 = 10.0;
/// 4px — the top edge a drag region leaves untagged, so the window can
/// still be resized from its top border. `SM_CYFRAME` is 4 logical pixels,
/// and gpui only reaches its own `HTTOP` fallback where no control area
/// answered first: a drag region flush to y = 0 would eat the resize edge
/// along the whole strip. A maximized window has no such edge and insets
/// nothing.
#[allow(dead_code)]
pub const CAPTION_RESIZE_EDGE: f32 = 4.0;
/// 32px — the Pane head's title row, inside the grounded header band.
pub const PANE_HEAD_H: f32 = 32.0;
/// The checkout line beneath the Pane head's title: 20px, sharing the
/// head's inline padding and its ground, so the two read as one band.
pub const PANE_CHECKOUT_H: f32 = 20.0;
/// The gap between the checkout line's own marks — tighter than the head's
/// gap, because these are one reading, not separate slots.
pub const CHECKOUT_GAP: f32 = 8.0;
/// 24px — the tasks strip.
pub const TASKS_STRIP_H: f32 = 24.0;
/// The tasks meter: 12 × 4 segments, 1px radius, 3px apart (15px pitch).
#[allow(dead_code)]
pub const METER_SEG_W: f32 = 12.0;
#[allow(dead_code)]
pub const METER_SEG_H: f32 = 4.0;
#[allow(dead_code)]
pub const METER_SEG_GAP: f32 = 3.0;
#[allow(dead_code)]
pub const METER_SEG_R: f32 = 1.0;
/// The checks card the header's `ci` mark opens (#29): wide enough for a
/// matrix job's own name — `test (windows-latest, stable)` — beside its
/// state word, which is the whole reason the card exists.
pub const CHECKS_CARD_W: f32 = 312.0;
pub const CHECKS_CARD_PAD: f32 = 8.0;
/// Between the card's heading and its runs.
pub const CHECKS_CARD_GAP: f32 = 8.0;
/// One run's line.
pub const CHECKS_ROW_H: f32 = 22.0;
/// A workflow's heading above the runs it owns, and the space that sets
/// that group off from the one before it.
pub const CHECKS_GROUP_H: f32 = 18.0;
pub const CHECKS_GROUP_GAP: f32 = 6.0;
pub const LED_WALL: f32 = 5.0;
pub const DONE_CELL_OPACITY: f32 = 0.75;
pub const DONE_WALL_OPACITY: f32 = 0.6;
// (end WP-C) — append above this line only

// ======================================== WP-D · composer, pickers, usage, draft
// Owner: WP-D (the Composer, its pickers and usage meter, attachments, background chips, the draft.)
// Edit values and append tokens only inside this section.

/// **The Composer is a raised block in the reading column.** Its outer edges
/// are the column's edges (`reading_column`), its content inset is
/// `BOX_INSET_X` (1px edge + `COMPOSER_PAD_X`), so its `❯` hangs in the same
/// glyph box as the transcript's and its text starts at the same C1. It is
/// `RAISED` with a 1px `COMPOSER_EDGE` that is always in layout; the edge
/// turns `FOCUS_RING` only when the Pane's own edge is a state colour and
/// the Composer holds the keyboard (otherwise the Pane ring, the accent `❯`
/// and the caret carry focus). Rows are `COMPOSER_ROW_H`, `COMPOSER_GAP`
/// apart: queued prompts (dim `❯` lines), the input line, the hint row.
/// Any pad, gap, edge or inset change here must update
/// `pane::composer_fixed_height` in the same commit.
pub const COMPOSER_PAD_X: f32 = BOX_INSET_X - 1.0;
pub const COMPOSER_PAD_T: f32 = SPACE_1_5;
pub const COMPOSER_PAD_B: f32 = SPACE_1_5;
pub const COMPOSER_ROW_H: f32 = 20.0;
pub const COMPOSER_GAP: f32 = SPACE_1;
/// The block's 1px edge, top and bottom: part of its fixed height.
pub const COMPOSER_EDGE_W: f32 = 1.0;
/// 8px — from the block to the Pane's bottom edge at L1 (the transcript's
/// own bottom padding supplies the air above it), and the L2 cell's inset
/// around its compact Composer on three sides. The block's 6px vertical
/// padding keeps a one-line Composer at 58px + this inset.
pub const COMPOSER_INSET_B: f32 = SPACE_2;
pub const COMPOSER_INSET_L2: f32 = SPACE_2;
/// The block's edge while the keyboard is in it on an alert Pane.
pub const COMPOSER_EDGE_FOCUS: u32 = FOCUS_RING;
/// Multiline drafts, controls and queued prompts share a bounded part of
/// the Pane, keeping most of its height available to the conversation.
pub const COMPOSER_MAX_PANE_FRACTION: f32 = 0.45;
/// The queued-prompt viewport scrolls beyond these visible row budgets.
pub const COMPOSER_QUEUE_ROWS: usize = 3;
pub const COMPOSER_COMPACT_QUEUE_ROWS: usize = 1;
/// 6px — between the shelf (pending files, background chips) and the block.
pub const SHELF_GAP: f32 = SPACE_1_5;
/// The caret: 2 × 16 in `CARET` (the accent), square, centred on integer
/// pixels in the 20px row (16 covers Geist Mono's ascender and descender at
/// `FS_UI`).
pub const CARET_W: f32 = 2.0;
pub const CARET_H: f32 = 16.0;
/// One selection colour app-wide: the Composer paints the transcript's
/// native selection wash under its selected runs.
pub const COMPOSER_SELECTION: u32 = TEXT_SELECTION_WASH;
/// An `@`-mention the operator picked: accent ink on the accent wash, the
/// inline-code ground family — visibly lighter than a selection.
pub const MENTION_INK: u32 = ACCENT;
pub const MENTION_WASH: u32 = ACCENT_WASH;
/// **Composer controls are quiet mono chips** (model, effort, mode, session
/// `•••`, the usage meter): `CHIP_H`, `PICKER_PAD_X` both sides, `R_CONTROL`,
/// no ground at rest, `FILL` under the pointer (the hover face on `RAISED`),
/// label `FS_SM` `TEXT_2`, a `ICON_CHEVRON_SM` chevron in `TEXT_MUTED`. A
/// busy control reads `TEXT_MUTED`, never faded. The model and effort pair
/// sits `PICKER_GAP` apart and reads as one unit.
pub const PICKER_PAD_X: f32 = SPACE_1_5;
pub const PICKER_GAP: f32 = SPACE_1;
pub const ICON_CHEVRON_SM: f32 = 10.0;
/// Send and Stop: quiet mono text controls, `COMPOSER_ROW_H` high with the
/// chip's inline padding.
pub const COMPOSER_ACTION_PAD_X: f32 = PICKER_PAD_X;
/// The context ring: a 14px box, 5.4px radius, 2px stroke, sweeping
/// clockwise from 12 o'clock with a round cap. No text, ever.
pub const USAGE_RING_D: f32 = 14.0;
#[allow(dead_code)]
pub const USAGE_RING_R: f32 = 5.4;
pub const USAGE_RING_W: f32 = 2.0;
/// The usage meter's detail card: one column of labelled bars, sized so
/// the three windows read at a glance without the card becoming a panel.
pub const USAGE_CARD_W: f32 = 216.0;
pub const USAGE_CARD_PAD: f32 = 10.0;
/// Between one window's block and the next, and inside one block.
pub const USAGE_CARD_GAP: f32 = 12.0;
pub const USAGE_CARD_ROW_GAP: f32 = 5.0;
pub const USAGE_CARD_BAR_H: f32 = 4.0;
/// Where a usage reading turns from neutral to ATTENTION, and from
/// ATTENTION to BLOCKED — a fraction of the window, not a count. Below
/// tight a meter is `TEXT_2`: colour is state, and a context half full is
/// not a state.
pub const USAGE_TIGHT: f32 = 0.75;
pub const USAGE_SPENT: f32 = 0.9;
/// Compact context / five-hour / weekly lines beside the `ctx 62%`
/// readout. The readout carries the precision, so the lines only need to
/// be glanceable.
pub const USAGE_LINE_W: f32 = 32.0;
pub const USAGE_LINE_H: f32 = 2.0;
pub const USAGE_LINE_GAP: f32 = 2.0;
/// Between the meter's three rings when the operator picks that mark:
/// tight enough that the trio reads as one control, wide enough that the
/// three readings stay separate.
pub const USAGE_RING_GAP: f32 = 4.0;
/// The readout's percent column: four mono cells at `FS_SM`, so 9% → 62% →
/// 100% never shifts the marks beside it.
pub const USAGE_READOUT_W: f32 = 4.0 * FS_SM * MONO_ADVANCE;
/// The session-controls card: permission modes, MCP servers and background
/// tasks as sections of menu rows, wide enough for a server's name beside
/// its state and two quiet actions.
pub const SESSION_CARD_W: f32 = 288.0;
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
/// A pending file on the shelf: a 22px chip with a 16px thumbnail or file
/// mark, the name cut at 200px.
pub const ATTACH_CHIP_H: f32 = 22.0;
pub const ATTACH_CHIP_MAX_W: f32 = 200.0;
pub const ATTACH_THUMB: f32 = 16.0;
// (end WP-D) — append above this line only

// ======================================== WP-E · menus, popovers, sheets, notifications
// Owner: WP-E (menus, popovers, Settings, the Project editor, notifications.)
// Edit values and append tokens only inside this section.

/// Form fields and segmented choices share a 32px row. Compact pane and
/// navigation controls keep their own smaller chrome metrics.
pub const FORM_CONTROL_H: f32 = 32.0;
/// Selected-value controls share a comfortable measure inside wider forms.
pub const FORM_FIELD_W: f32 = 320.0;
/// Inset around the chips of a segmented choice control.
pub const FORM_CHOICE_PAD: f32 = 3.0;
/// Settings and Project editors share the same header and content insets.
pub const MODAL_HEAD_H: f32 = 48.0;
pub const MODAL_PAD: f32 = 16.0;
pub const MODAL_GAP: f32 = 12.0;
/// Editors leave an even breathing edge while making room for a scrolling
/// form at short desktop heights.
pub const MODAL_VIEWPORT_FRACTION: f32 = 0.92;
// (end WP-E) — append above this line only

// ======================================== WP-F · decisions and subagents
// Owner: WP-F (decision.rs, subagents.rs, the Decision card and keycaps.)
// Edit values and append tokens only inside this section.

/// The Decision card: 12px inline margin, 8px below, 8/10 padding, a 10px
/// gap, and a 15px warning mark.
#[allow(dead_code)]
pub const DECISION_MARGIN_X: f32 = 12.0;
#[allow(dead_code)]
pub const DECISION_MARGIN_B: f32 = 8.0;
#[allow(dead_code)]
pub const DECISION_PAD_X: f32 = 10.0;
#[allow(dead_code)]
pub const DECISION_PAD_Y: f32 = 8.0;
#[allow(dead_code)]
pub const DECISION_GAP: f32 = 10.0;
pub const ICON_WARNING: f32 = 15.0;
/// The Decision card's keycaps today: 3px block, 7px inline padding.
#[allow(dead_code)]
pub const KEYCAP_PAD_X: f32 = 7.0;
#[allow(dead_code)]
pub const KEYCAP_PAD_Y: f32 = 3.0;
/// The native checkbox/radio indicator's box.
const CHOICE_CONTROL: f32 = 16.0;
/// The lift that drops a question choice's label onto the native
/// checkbox/radio indicator's center. The control top-aligns with the label
/// column, whose first line boxes `LH_PROSE_SM`, so the label rides half that
/// difference too low. Lifting the label rather than sinking the control
/// keeps a wrapped choice and its description flowing from the same edge.
pub const CHOICE_LABEL_LIFT: f32 = (LH_PROSE_SM - CHOICE_CONTROL) / 2.0;
// (end WP-F) — append above this line only

// ======================================== WP-G · nav
// Owner: WP-G (nav.rs and its cockpit wiring.)
// Edit values and append tokens only inside this section.

/// A nav row that will accept the drag.
#[allow(dead_code)]
pub const DROP_VALID: u32 = ACCENT;
/// A nav row that refuses the drag.
#[allow(dead_code)]
pub const DROP_REFUSED: u32 = BLOCKED;
/// 42px — the nav head band, which holds the Project filter.
#[allow(dead_code)]
pub const NAV_HEAD_H: f32 = 42.0;
/// The nav tree's padding: 8px top and inline, 16px bottom.
#[allow(dead_code)]
pub const NAV_TREE_PAD: f32 = 8.0;
#[allow(dead_code)]
pub const NAV_TREE_PAD_B: f32 = 16.0;
/// 14px — the nav filter trigger's chevron.
#[allow(dead_code)]
pub const ICON_CHEVRON_LG: f32 = 14.0;
/// 28px — the Project filter trigger.
#[allow(dead_code)]
pub const FILTER_TRIGGER_H: f32 = 28.0;
/// 38px — the filter menu's offset below the nav head's top edge.
#[allow(dead_code)]
pub const MENU_TOP: f32 = 38.0;
/// 254px — the content box of a root-level nav row: the column less the
/// tree's inline padding, less the row's own. A truncating title has to be
/// pinned to it, because gpui only measures an ellipsis against a width it
/// knows on the line's very first measure (see `nav::group_row`).
#[allow(dead_code)]
pub const ROW_TEXT_W: f32 = NAV_WIDTH - 2.0 * NAV_TREE_PAD - 2.0 * ROW_PAD_X;
/// 16px between Group blocks; 6px between a Group row and its members;
/// 2px between sibling rows; 24px above the solo section.
#[allow(dead_code)]
pub const GROUP_GAP: f32 = 16.0;
#[allow(dead_code)]
pub const MEMBERS_TOP: f32 = 6.0;
#[allow(dead_code)]
pub const MEMBER_GAP: f32 = 2.0;
#[allow(dead_code)]
pub const SOLOS_TOP: f32 = 24.0;
/// The member indent: rows move 20px right, and the 1px rail sits 7px left
/// of them (13px right of the Group row's own edge), inset 3px top and
/// bottom of the members box.
#[allow(dead_code)]
pub const MEMBER_INDENT: f32 = 20.0;
#[allow(dead_code)]
pub const RAIL_OFFSET: f32 = 7.0;
#[allow(dead_code)]
pub const RAIL_INSET: f32 = 3.0;
/// 14px — the provider logomark in a nav row.
#[allow(dead_code)]
pub const PROVIDER_MARK: f32 = 14.0;
// (end WP-G) — append above this line only

#[cfg(test)]
mod tests {
    use super::*;

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

    const PLANES: &[(&str, u32)] = &[
        ("GROUND", GROUND),
        ("PANE", PANE),
        ("RAISED", RAISED),
        ("RAISED_2", RAISED_2),
        ("HOVER", HOVER),
        ("FILL", FILL),
    ];

    #[test]
    fn ink_clears_its_floor_on_every_plane() {
        // Readable text: AA body text on every plane, selected rows included.
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
        // Structure only: non-text 3:1 on the planes it draws on.
        floor(
            &[("TEXT_FAINT", TEXT_FAINT)],
            &[("GROUND", GROUND), ("PANE", PANE)],
            3.0,
        );
        // Everything a code block or a raised card writes.
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
                ("SYN_COMMENT", SYN_COMMENT),
                ("SYN_PUNCT", SYN_PUNCT),
                ("SYN_PLAIN", SYN_PLAIN),
                ("DIFF_ADDED_INK", DIFF_ADDED_INK),
                ("DIFF_REMOVED_INK", DIFF_REMOVED_INK),
            ],
            &[("PANE", PANE), ("RAISED", RAISED)],
            4.5,
        );
        // A primary button's label, at rest and under the pointer.
        floor(
            &[("ON_ACCENT", ON_ACCENT)],
            &[
                ("ACCENT_STRONG", ACCENT_STRONG),
                ("PRIMARY_HOVER", PRIMARY_HOVER),
                ("PRIMARY_ACTIVE", PRIMARY_ACTIVE),
            ],
            4.5,
        );
        // Focus and control boundaries: non-text 3:1.
        floor(
            &[("FOCUS_RING", FOCUS_RING), ("INPUT_EDGE", INPUT_EDGE)],
            &[("GROUND", GROUND), ("PANE", PANE), ("RAISED", RAISED)],
            3.0,
        );
        // Ink stays readable on the washes painted under it.
        floor(
            &[("TEXT_STRONG", TEXT_STRONG)],
            &[("ACCENT_WASH on PANE", over(ACCENT_WASH, PANE))],
            4.5,
        );
        floor(
            &[("TEXT", TEXT)],
            &[
                (
                    "TEXT_SELECTION_WASH on PANE",
                    over(TEXT_SELECTION_WASH, PANE),
                ),
                ("SELECTION", SELECTION),
            ],
            4.5,
        );
    }

    #[test]
    fn the_ink_ladder_steps_down() {
        let ladder = [TEXT_STRONG, TEXT, TEXT_2, TEXT_MUTED, TEXT_FAINT];
        for pair in ladder.windows(2) {
            assert!(luminance(pair[0]) > luminance(pair[1]), "{pair:06x?}");
        }
    }

    #[test]
    fn elevation_ladder_is_strictly_ordered() {
        let ladder = [
            ("GROUND", GROUND),
            ("PANE", PANE),
            ("RAISED", RAISED),
            ("RAISED_2", RAISED_2),
            ("FILL", FILL),
            ("FILL_HOVER", FILL_HOVER),
        ];
        for pair in ladder.windows(2) {
            assert!(
                luminance(pair[0].1) < luminance(pair[1].1),
                "{} must sit below {}",
                pair[0].0,
                pair[1].0
            );
        }
        // The hover face on the low planes sits between them and the chips.
        assert!(luminance(PANE) < luminance(HOVER));
        assert!(luminance(HOVER) < luminance(RAISED_2));
        // Every hover face is a visible step off the plane it hovers on.
        for (face, plane) in [(HOVER, GROUND), (HOVER, PANE), (FILL, RAISED)] {
            assert!(contrast(face, plane) >= 1.1, "{face:06x} on {plane:06x}");
        }
        // Faces are opaque: the no-bleed rule of `pointer.rs`.
        for face in [GROUND, PANE, RAISED, RAISED_2, HOVER, FILL, FILL_HOVER] {
            assert!(face <= 0xffffff, "{face:x} carries alpha");
        }
    }

    #[test]
    fn nav_row_heights_are_derived() {
        assert_eq!(THREAD_ROW_H, 2.0 * ROW_PAD_Y + LH_TIGHT + ROW_GAP + LH_META);
        assert_eq!(GROUP_ROW_H, THREAD_ROW_H);
        assert_eq!(THREAD_ROW_H, 44.0);
    }

    #[test]
    fn every_type_role_has_a_whole_pixel_line_box() {
        use ferrite_core::settings::SoloReadingSize;
        for (size, line) in [
            (FS_PROSE, LH_PROSE),
            (FS_PROSE_SM, LH_PROSE_SM),
            (FS_UI, LH_UI),
            (FS_UI, LH_CODE),
            (FS_SM, LH_META),
            (FS_UI, LH_TIGHT),
        ] {
            assert_eq!(line, line.round());
            assert!(line >= size * 1.25, "{size}px on a {line}px line box");
        }
        for reading in [
            SoloReadingSize::Standard,
            SoloReadingSize::Comfortable,
            SoloReadingSize::Large,
        ] {
            let (size, line) = (answer_text_size(reading), answer_line_height(reading));
            assert_eq!(line, line.round());
            assert!(line >= size * 1.5, "{reading:?}: {size}/{line}");
        }
        assert_eq!(answer_text_size(SoloReadingSize::Standard), FS_PROSE);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(near(FS_PROSE * heading_scale(1), 18.0));
        assert!(near(FS_PROSE * heading_scale(2), 16.0));
        assert!(near(FS_PROSE * heading_scale(3), FS_PROSE));
        assert_eq!(prose_line_height(FS_PROSE), LH_PROSE);
        assert!(near(MONO_CELL, 7.5));
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
            assert_eq!(tokens.accent.color, solid(FILL));
            assert_eq!(tokens.popover.color, solid(MENU));
            assert_eq!(tokens.muted.color, solid(RAISED));
            assert_eq!(tokens.ring.color, solid(FOCUS_RING));
            assert_eq!(tokens.input.color, solid(INPUT_EDGE));
            assert_eq!(tokens.border.color, alpha(HAIRLINE_STRONG));
            assert_eq!(tokens.selection.color, alpha(TEXT_SELECTION_WASH));
            assert_eq!(tokens.caret.color, solid(ACCENT));
            assert_eq!(tokens.link.color, solid(ACCENT));
            assert_eq!(tokens.sidebar.color, solid(GROUND));
            // And the colours the non-token paths read.
            assert_eq!(theme.primary, solid(ACCENT_STRONG));
            assert_eq!(theme.muted, solid(RAISED));
            assert!(!theme.shadow, "floats wear Ferrite's own shadow recipe");
            assert_eq!(theme.radius, gpui::px(R_CONTROL));
            assert_eq!(theme.radius_lg, gpui::px(R_BLOCK));
            assert_eq!(theme.font_family.as_ref(), FONT_UI);
            assert_eq!(theme.mono_font_family.as_ref(), FONT_MONO);
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

    #[test]
    fn faces_are_the_bundled_families() {
        assert_eq!(FONT_UI, FONT_MONO);
        for face in crate::FONTS {
            let family = family(face);
            assert!(
                family == FONT_MONO || family == FONT_PROSE,
                "a bundled face names the family `{family}`"
            );
        }
        assert_eq!(family(GEIST_MONO), FONT_MONO);
    }

    #[test]
    fn chrome_glyphs_are_in_geist_mono() {
        assert!(covers(GEIST_MONO, 'a') && covers(GEIST_MONO, '$'));
        // The glyphs the grammar must draw as SVG, because the face lacks them.
        for missing in ['❯', '⎿', '∴', '✻', '✓', '✗', '☐'] {
            assert!(!covers(GEIST_MONO, missing), "{missing} is covered now");
        }
        for glyph in CHROME_GLYPHS {
            assert!(covers(GEIST_MONO, *glyph), "{glyph} is not in Geist Mono");
        }
    }

    /// Every non-ASCII glyph render code puts in a literal must be one the
    /// bundled mono face draws. Scans the render modules' non-test source,
    /// skipping comments.
    ///
    /// TODO(work packages): after F1 these surfaces still draw glyphs Geist
    /// Mono lacks as text (`--include-ignored` lists them). Each owner makes
    /// them SVG glyph boxes (or a covered glyph), then removes the `#[ignore]`:
    /// - WP-A (transcript): the prompt `❯` (pane.rs `render_block`), the `⎿`
    ///   result elbows (`output_block`, `result_line`), and the `⎿` in the
    ///   transcript copy formatter (cockpit.rs, before `mod tests`).
    /// - WP-C (pane frame, L2, wall, board): `◐`/`✗`/`✓`/`⚠` state marks and
    ///   `❯ idle` in `wall_state`/`wall_cell`/`l2_cell`, the ci mark's `✗`,
    ///   the tasks meter's `▰`/`▱`, and the pane-drop `⇄ Swap` label.
    /// - WP-D (composer, draft): the draft band's `⌵` chevron and the queued
    ///   line's `⏳`.
    /// - WP-E (menus, sheets): `⌘` in context-menu hints and in the Settings
    ///   "⌘B toggles it" description.
    /// - WP-F (decisions): `⌘` in the "Expand to answer" tooltip.
    #[test]
    #[ignore = "enabled by WP-A/WP-C/WP-D/WP-E/WP-G once their glyphs are SVG (see the doc TODO)"]
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
                        missing.push(format!("{file}:{} {c} (U+{:04X})", at + 1, c as u32));
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "glyphs Geist Mono lacks:\n{}",
            missing.join("\n")
        );
    }
}
