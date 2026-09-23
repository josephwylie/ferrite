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
//!    icon's hue. The accent (`ACCENT` and its family) is that hue with only
//!    a little more chroma — a pale steel that reads as a tint, never as
//!    "blue" — and it marks the prompt `❯`, the caret, links, focus,
//!    selection and primary actions. Nothing else is tinted.
//!    The whole palette is quiet: saturation stays at or under ~45% (state)
//!    and ~22% (everything else), so the window reads as grey with signals.
//! 2. **Colour is state.** `RUNNING`, `ATTENTION` and `BLOCKED` mark status
//!    only. A failure colours the word that says so, never the whole row.
//!    Green never means "finished". Provider logomarks are monochrome
//!    everywhere, the picker included.
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
//! 6. **Two faces, chosen by what the text is.** `FONT_UI` (Geist) is the
//!    face of the app: the nav, the titlebar, Pane and cell heads, the prompt
//!    echo (set apart by its `❯`, weight and ink, not its face), group
//!    summaries, stamps, the working line, Decisions, menus, sheets,
//!    notifications, chips, buttons, empty states, and agent prose.
//!    `FONT_CODE` (Geist Mono) is only for literal code and machine text:
//!    fenced blocks and inline `code`, diffs and their number column, a tool
//!    call's arguments and every line of its output (the tool's name is UI),
//!    the Composer's input line, placeholder and queued prompts (a terminal
//!    line), a Decision's command well, keycaps, and the aligned `/command`
//!    names. A metric that assumes a fixed advance (`CODE_CELL`) is only ever
//!    laid out against code text. Weights: 400 body; 500 a surface's single
//!    title, labels and the prompt echo; 600 prose emphasis (headings,
//!    `**strong**`, the Decision question); 700 is unused.
//! 7. **Pixel line heights.** Every text role is a (size, line height) pair,
//!    and fixed row heights are `const` expressions of those pairs, never
//!    hand-summed literals.
//! 8. **A space scale:** 2 · 4 · 6 · 8 · 12 · 16 · 24 · 32 (`SPACE_*`,
//!    named in gpui's 4px units). A metric off the scale says why in its doc.
//!    More space above a heading or a new turn than below it.
//! 9. **Radii say role, not size:** Pane 10 · block 8 · control 6 · chip 4 ·
//!    tight 3. A nested radius is the outer radius less its inset.
//! 10. **Glyph coverage.** A glyph outside either bundled face's cmap is
//!     never text on any surface; it is an SVG in a glyph box. `CHROME_GLYPHS`
//!     lists the non-ASCII glyphs text may use, and a test checks each against
//!     both bundled faces, Geist and Geist Mono.
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
/// The Composer's resting edge.
pub const COMPOSER_EDGE: u32 = HAIRLINE_STRONG;
/// Rules between transcript table rows.
pub const TABLE_RULE: u32 = HAIRLINE;
/// `#696f78` — a resting checkbox, radio or switch boundary: solid and at
/// least 3:1 on `PANE` and `RAISED`, so an unchecked control never vanishes.
pub const INPUT_EDGE: u32 = 0x696f78;
/// `#2f3339` / `#43484f` — the scrollbar thumb, at rest and under the pointer.
pub const SCROLLBAR: u32 = 0x2f3339;
pub const SCROLLBAR_HOVER: u32 = 0x43484f;
/// `#ffffff1a` — an unlit tasks-meter segment and the usage lines' tracks.
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

/// `#afbaca` — pale steel at the app icon's hue (HSL 216°, 20%): the prompt
/// `❯`, links, the active indicator, a selected check, a drop target. The
/// icon's own stops run 34–42% saturated; the accent stays under them so the
/// chrome never out-colours the logo.
pub const ACCENT: u32 = 0xafbaca;
/// `#cdd4df` — the accent a step lighter: link hover, accent on `FILL` or on
/// a selection.
pub const ACCENT_HI: u32 = 0xcdd4df;
/// `#4f5d72` — the accent as a fill: the primary button (white ink 6.7:1).
pub const ACCENT_STRONG: u32 = 0x4f5d72;
/// A primary button under the pointer and held down.
pub const PRIMARY_HOVER: u32 = 0x5a6a81;
pub const PRIMARY_ACTIVE: u32 = 0x475466;
/// Ink on an `ACCENT_STRONG` fill.
pub const ON_ACCENT: u32 = 0xffffff;
/// `#7d8ba1` — **the** keyboard-focus ink: the focused Pane's ring, the kit's
/// `ring`, every focus outline. At least 3:1 on `GROUND` and `PANE`.
pub const FOCUS_RING: u32 = 0x7d8ba1;
/// `#afbaca66` — the accent as an outline that is not focus: a link's
/// underline, a selected choice's edge, a drop target's edge.
pub const ACCENT_EDGE: u32 = 0xafbaca66;
/// `#afbaca24` (14%) — the accent as a ground: inline code, a selected accent
/// row, the slot a dragged Pane would take.
pub const ACCENT_WASH: u32 = 0xafbaca24;
/// `#afbaca40` (25%) — native text selection, painted over glyphs.
pub const TEXT_SELECTION_WASH: u32 = 0xafbaca40;
/// The caret.
pub const CARET: u32 = ACCENT;

// -------------------------------------------------------- state + signals

/// `#8cb59d` — live work (a sage, 22%): the running status dot, a running signal line, the
/// pass chip, diff `+`.
pub const RUNNING: u32 = 0x8cb59d;
/// Running as a ground: an added hunk row, the pass chip.
pub const RUNNING_WASH: u32 = 0x8cb59d1f;
/// The halo that breathes behind a working Thread's dot in the nav.
pub const RUNNING_HALO: u32 = 0x8cb59d59;
/// `#cbb280` — a Decision (a muted ochre, 42%): the status dot, the signal line, the Pane's edge,
/// the Decision card's mark.
pub const ATTENTION: u32 = 0xcbb280;
/// A Decision card's ground.
pub const ATTENTION_WASH: u32 = 0xcbb28014;
/// A Decision card's 1px inset ring. An inset ring takes no layout.
pub const ATTENTION_EDGE: u32 = 0xcbb28059;
/// `#d29089` — blocked or failed (a dusty red, 45%): the status dot, the signal line, the
/// Pane's edge, diff `−`, the word "failed".
pub const BLOCKED: u32 = 0xd29089;
/// Blocked as a ground: a removed hunk row.
pub const BLOCKED_WASH: u32 = 0xd290891f;
/// The idle/parked status dot: the muted ink in a dot role.
pub const IDLE: u32 = TEXT_MUTED;

/// Provider marks are monochrome: the glyph's shape tells the providers
/// apart, so no brand colour enters the chrome.
pub const PROVIDER_CODEX: u32 = TEXT_2;
pub const PROVIDER_CLAUDE: u32 = TEXT_2;

// ------------------------------------------------------- transcript colour

/// Syntax is near-monochrome: each class is a faint tint (≤ 22%) at a
/// distinct lightness, so structure reads without the block turning into a
/// colour chart, and code never reads as state.
/// Keywords: the accent's steel.
pub const SYN_KEYWORD: u32 = 0xc3cad5;
/// Function names: nearly `TEXT_STRONG`.
pub const SYN_FUNCTION: u32 = 0xd8dbdf;
/// Type names: a cooler steel, apart from keywords.
pub const SYN_TYPE: u32 = 0xbfcacf;
/// String literals: a grey-green, far quieter than `RUNNING`.
pub const SYN_STRING: u32 = 0xaec2b1;
/// Number literals: a warm grey, never read as a Decision.
pub const SYN_NUMBER: u32 = 0xcbbdae;
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

/// The float shadow (`components::float_shadow`), two layers under every
/// floating surface. Blurs are gpui's gaussian σ, half a CSS blur: the far
/// layer is CSS `0 8px 24px -4px` at 55%, its −4 spread keeping it *under*
/// the surface rather than a halo around it; the near layer is a CSS
/// `0 1px 3px` contact line at 40%. On the near-black ground the hairline
/// edge carries the elevation and the shadow only lifts the surface off
/// the Panes.
pub const SHADOW_FAR: u32 = 0x0000008c;
pub const SHADOW_FAR_Y: f32 = 8.0;
pub const SHADOW_FAR_BLUR: f32 = 12.0;
pub const SHADOW_FAR_SPREAD: f32 = -4.0;
pub const SHADOW_NEAR: u32 = 0x00000066;
pub const SHADOW_NEAR_Y: f32 = 1.0;
pub const SHADOW_NEAR_BLUR: f32 = 1.5;

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
/// per-character cell of *code* text must be laid out on so it cannot round
/// up to a whole pixel. UI text is proportional: nothing lays it out on a
/// cell.
pub const CODE_ADVANCE: f32 = 0.6;
/// 7.5px — one code column at `FS_UI`.
pub const CODE_CELL: f32 = FS_UI * CODE_ADVANCE;
/// 0.5em — a floor under Geist's average advance in UI copy. Only an
/// estimate from below may be laid out against proportional text (a title's
/// floor), so a short label is never padded past itself.
pub const UI_ADVANCE_FLOOR: f32 = 0.5;
/// 4px — one word space between UI runs laid side by side (a caption and its
/// facts, a summary and its ` · N failed`).
pub const WORD_GAP: f32 = SPACE_1;

/// The non-ASCII glyphs text may use in either face: every one is in both
/// bundled cmaps (asserted by `theme::tests`). Anything else — `❯ ⎿ ∴ ✻ ✓ ✗ ☐`
/// and friends — is an SVG in a glyph box, never text. A rule the tests
/// enforce, so it compiles only with them.
#[cfg(test)]
pub const CHROME_GLYPHS: &[char] = &[
    '↳', '±', '↑', '↓', '⇥', '⇧', '↵', '•', '●', '…', '→', '·', '−', '—', '›',
];
/// The glyphs only keys may use: a key is code text (rule 6), drawn in the
/// code face wherever it appears (`components::key_hints`, keycaps), and
/// Geist has no `⌫`.
#[cfg(test)]
pub const KEY_GLYPHS: &[char] = &['⌫'];

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
pub const NAV_WIDTH: f32 = 286.0;
pub const NAV_RAIL_WIDTH: f32 = if cfg!(target_os = "macos") {
    TRAFFIC_RESERVE
} else {
    56.0
};
/// 42px — the window-chrome band across the top of the window. Over the nav
/// it is the column's own chrome row (the traffic-light reserve and the
/// collapse button, `nav::win_chrome`); over the board it is the titlebar
/// strip (location, `dev` tag, add control; on Windows the caption buttons,
/// `titlebar::strip`), an overlay that adds no layout. The Pane board
/// starts under it, at `BOARD_TOP`.
pub const WIN_CHROME_H: f32 = 42.0;
/// 77px — the horizontal room the window-chrome band reserves before the
/// collapse button: the traffic lights plus the prototype's 8px flex gap
/// and 4px button margin. Measured from the prototype (button left edge
/// x = 77). On macOS the *host* lights occupy it; nothing else may be drawn
/// there, and nothing interactive may sit in the band's top 28px or AppKit's
/// native drag region stops working.
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
/// A toast's width: the nav column less 8px each side, so a toast stacked at
/// the foot of the nav covers only the ground and never a Pane.
pub const TOAST_W: f32 = NAV_WIDTH - 2.0 * SPACE_2;
/// One toast's height: 12px padding around a title line over a meta line,
/// inside its 1px edge. The kit stacks at most `TOAST_LAYERS` of them,
/// each layer behind the front peeking `TOAST_PEEK` above it (the kit's
/// collapsed-stack geometry).
pub const TOAST_H: f32 = 2.0 * SPACE_3 + LH_UI + LH_META + 2.0;
pub const TOAST_PEEK: f32 = 14.0;
pub const TOAST_LAYERS: usize = 3;
/// What the foot of the nav gives up while `layers` toasts are stacked on
/// it: the stack, its bottom margin and 8px of air, so nothing in the nav
/// (the Parked fold above all) ever sits under a toast.
pub const fn toast_reserve(layers: usize) -> f32 {
    if layers == 0 {
        return 0.0;
    }
    let layers = if layers < TOAST_LAYERS {
        layers
    } else {
        TOAST_LAYERS
    };
    GRID_PAD + TOAST_H + TOAST_PEEK * (layers - 1) as f32 + SPACE_2
}

// -------------------------------------------- pane body and the row column

/// 16px — the inline padding every Pane strip shares.
pub const PANE_PAD_X: f32 = SPACE_4;
/// The Pane body's padding: 16px top, so the first line never kisses the
/// head rule, and 32px bottom, the room the working line overlays.
pub const BODY_PAD_T: f32 = SPACE_4;
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
pub const ELBOW_INDENT: f32 = GUTTER_W;
/// 13px — a raised box's content inset (1px edge + 12px padding). Transcript
/// rows sit the same distance inside the reading column, so the transcript `❯`
/// and the Composer `❯` share one axis.
pub const BOX_INSET_X: f32 = 13.0;
/// 8px — between a row's glyph column and its text (tool rows, the working
/// line, the turn diff, controls beside a label).
pub const EVENT_GAP: f32 = 8.0;
/// 1px — the focused Pane's ring (`FOCUS_RING` ink), lying exactly on the
/// Pane's own border box: focus changes colour and nothing else. It is an
/// absolutely positioned overlay inside a non-clipping wrapper, since a ring
/// drawn inside the shell's `overflow_hidden()` would be clipped.
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
/// 10px — the drawn `⌘` in a key combination (`components::key_combo`):
/// the `FS_SM` cap-height band, so it sits on the letters beside it.
pub const KEY_GLYPH: f32 = 10.0;
/// A keycap: 18px high (it fits inside a 20px UI row), 5px inline padding.
pub const KBD_H: f32 = 18.0;
pub const KBD_PAD_X: f32 = 5.0;
/// A chip: 20px high, 6px inline padding — the mode chip, the pass chip, the
/// changed strip's file chips, background tasks.
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
pub const ROW_PAD_X: f32 = 8.0;
pub const ROW_PAD_Y: f32 = 6.0;
pub const ROW_GAP: f32 = 0.0;
/// 44px — a Thread row: its padding around a title line over a meta line,
/// 6 + 16 + 0 + 16 + 6. Derived from the type, never summed by hand.
pub const THREAD_ROW_H: f32 = 2.0 * ROW_PAD_Y + LH_TIGHT + ROW_GAP + LH_META;
/// A Group parent row: the same two lines, so the same 44px.
pub const GROUP_ROW_H: f32 = THREAD_ROW_H;
/// The folder and branch marks on the Project and checkout lines (12px), and
/// the 5px gap to their labels.
pub const ROW_ICON: f32 = 12.0;
pub const ROW_ICON_GAP: f32 = 5.0;
/// 12px — the provider logomark in a picker row and the Composer's chip.
pub const PROVIDER_MARK_SM: f32 = 12.0;
/// The badge under the pointer while a Pane or a nav row is dragged
/// (`components::drag_badge`): 26px high, 10px inline padding, at most
/// 280px before the title truncates.
pub const DRAG_BADGE_H: f32 = 26.0;
pub const DRAG_BADGE_PAD_X: f32 = 10.0;
pub const DRAG_BADGE_MAX_W: f32 = 280.0;
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

// ------------------------------------------------------------------ faces

/// The UI face, **bundled**: Geist. Everything an operator reads as the app
/// — chrome, heads, menus, sheets, summaries — and agent prose (rule 6).
/// The kit's `font_family` is set from it.
///
/// gpui has no variation-axis support, so `main.rs` registers static
/// instances (Regular, Italic, Medium, SemiBold, Bold) of both faces. They
/// share their typographic family name (name ID 16), and CoreText/DirectWrite
/// resolve the face from `.font_weight(..)`. **Never reach a weight by
/// family name**: `.font_family("Geist Medium")` silently resolves to the
/// fallback face.
pub const FONT_UI: &str = "Geist";

/// The code face, bundled: Geist Mono. Only literal code and machine text
/// (rule 6); the kit's `mono_font_family` is set from it.
pub const FONT_CODE: &str = "Geist Mono";

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
    theme.mono_font_family = FONT_CODE.into();
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
    // Kit tabs, should one appear (the Subject strip draws its own on the
    // headless tab): no bar ground, the active tab a FILL pill in
    // TEXT_STRONG, the rest muted.
    theme.tab_bar = rgba(TRANSPARENT).into();
    theme.tab_active = rgb(FILL).into();
    theme.tab_active_foreground = rgb(TEXT_STRONG).into();
    theme.tab_foreground = rgb(TEXT_MUTED).into();
    // `Theme::change` resolved the kit's pre-computed tokens from its default
    // palette, and nothing recomputes them: widgets that read `tokens.*`
    // (Button::primary, menu rows, tooltips, checkboxes) would paint the
    // kit's neutrals. Rebuild them from the colours above.
    theme.tokens = gpui::component::ThemeTokens::from(&theme.colors);
    // Toasts stack at the foot of the nav column, 8px in from its edges:
    // the ground is the least valuable space, so no toast covers a Pane's
    // head or its Composer. With the nav collapsed the cockpit moves the
    // stack BottomRight, above the Composer (`present_notices`). Five at
    // once is a wall's worth; the bell holds the rest.
    theme.notification.placement = gpui::Anchor::BottomLeft;
    theme.notification.margins = gpui::base::Edges {
        top: px(BOARD_TOP),
        right: px(GRID_PAD),
        bottom: px(GRID_PAD),
        left: px(SPACE_2),
    };
    theme.notification.width = px(TOAST_W);
    theme.notification.max_items = 5;
}

// ======================================== end of the frozen shared head

// ======================================== WP-A · transcript rows and grammar
// Owner: WP-A (transcript.rs, pane/text.rs, the transcript rows in pane.rs, ferrite-core transcript strings.)
// Edit values and append tokens only inside this section.

// ---------------------------------------------------- transcript grammar
//
// The transcript is Claude Code's layout in Ferrite's ink:
//
// - **One content edge.** Every row is `[gutter | text]`: a `GLYPH_BOX`
//   glyph centred on the row's first line box, `GUTTER_GAP`, then text at
//   C1 (`GUTTER_W`). Prompt text, answer prose, tool calls, group summaries,
//   reasoning and the turn stamp all start at C1. A result hangs one gutter
//   further in, at C2, under a drawn elbow whose stem sits under the call's
//   name. Rows are inset `BOX_INSET_X` inside the reading column, so the
//   transcript `❯` and the Composer's share one axis.
// - **Glyphs are drawn, never typed.** `❯` is `prompt.svg`, `∴` is
//   `reasoning.svg`, the answer mark is the monochrome `ferrite-mono.svg`,
//   the tool dot and the elbow are painted. None of them registers text.
// - **One left edge.** A disclosure's chevron leads, in the gutter where
//   tool dots hang; nothing sits at the reading column's right but a
//   settled call's duration.
// - **State lives in the dot.** A tool's name is neutral ink whatever
//   happened; its dot says how it went (`tool_dot`), and a failure colours
//   the one word that says so. A collapsed group is one muted line whose only
//   state ink is ` · N failed`.
// - **Rhythm in three steps.** A turn opens `GAP_TURN` (32) under the one
//   before it; blocks inside a turn — answer, summary, tool row, stamp —
//   sit `GAP_SECTION` (12, the block step) apart; rows of one run of work
//   (and a one-paragraph commentary into the call it introduces) sit
//   `GAP_TOOL` (4, the row step) apart. The space above a row is chosen
//   once, at reconcile, from the row before it and its own kind, and is
//   part of the row's identity, so a changed gap is a changed row and
//   nothing is measured per frame.
// - **The prompt anchors its turn.** The operator's line is the turn's
//   heading: prose size (`FS_PROSE`/`LH_PROSE`) at `W_LABEL` in
//   `TEXT_STRONG` under the accent `❯`, over answers at prose size, regular,
//   in `TEXT`. Structural rows (tool calls, summaries) are `FS_UI`/`LH_UI`;
//   the stamp and the trail are `FS_SM`/`LH_META`.

/// 32px — above every prompt but the first: the turn boundary. No rule is
/// drawn between turns; this space, the prompt's weight and the stamp do the
/// job.
pub const GAP_TURN: f32 = SPACE_8;
/// 12px — a change of voice: prompt → the agent's first row, prose ↔ tools,
/// anything ↔ reasoning, notices, the turn's changes.
pub const GAP_SECTION: f32 = SPACE_3;
/// 4px — tool rows in one run of work, and a one-paragraph commentary that
/// introduces the tool row under it.
pub const GAP_TOOL: f32 = SPACE_1;
/// The last row of a turn → its stamp (and a decision record under the row
/// it answers): the block step, like any block of the turn.
pub const GAP_STAMP: f32 = GAP_SECTION;

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

/// An added diff line's code: `RUNNING` lifted a step to read on its wash.
pub const DIFF_ADDED_INK: u32 = 0xb4cfc0;
/// A removed diff line's code: `BLOCKED` lifted the same step.
pub const DIFF_REMOVED_INK: u32 = 0xddb5b0;
/// A diff card at C2: `RAISED`, `R_CHIP`, 4px above and inside it, 8px
/// inline. Its columns are `[number][8][sign][4][code]`: the number column
/// is as wide as the largest number's digits (`CODE_CELL` each), the sign is
/// one whole-pixel mono cell, and code keeps its indentation.
pub const HUNK_PAD_X: f32 = SPACE_2;
pub const HUNK_PAD_Y: f32 = SPACE_1;
pub const HUNK_MARGIN_T: f32 = SPACE_1;
pub const DIFF_SIGN_W: f32 = SPACE_2;
pub const DIFF_GAP: f32 = SPACE_2;
pub const DIFF_SIGN_GAP: f32 = SPACE_1;
/// How many rows one hunk card draws before it stops and says how many it
/// did not. An edit's patch is a handful of lines; a written file's is
/// however long the file is, and a card that redrew a 900-line file would
/// be the transcript rather than a note in it.
pub const HUNK_MAX_ROWS: usize = 24;

/// 20px — an invisible hit area, not a drawn thing: a disclosure's leading
/// chevron target (the gutter, `GUTTER_W`) and a prompt action's button.
pub const TOOL_DISCLOSURE_HIT: f32 = GUTTER_W;
/// 10px — the leading disclosure chevron, in the gutter's glyph box.
pub const DISCLOSURE_CHEVRON: f32 = 10.0;
/// 4px — how far a prompt's hover wash bleeds past its text on each side.
pub const PROMPT_HOVER_BLEED: f32 = SPACE_1;
/// 16px — a fallback list item's hang: `-` at C1, text 16px in.
pub const UL_INDENT: f32 = SPACE_4;
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
//
// **The Pane frame.** A Pane is a `PANE` sheet with a 1px edge that is always
// in layout, so a state change recolours it and nothing reflows. The edge
// says one thing, by precedence (`pane::PaneEdge`): blocked `BLOCKED` >
// a Decision `ATTENTION` > focused `FOCUS_RING` > at rest `HAIRLINE`, which
// lifts to `HAIRLINE_STRONG` under the pointer. Focus is drawn only while
// more than one Pane is on the board: a lone Pane is plainly the one with
// the keyboard and rests on its hairline. A *focused* alert Pane beside
// others also draws a `FOCUS_RING` ring inset by 2px, so focus is never
// hidden by a state. A Thread that finished while the operator looked elsewhere breathes
// an `ACCENT` ring until they land on it (still under reduced motion).
//
// **The head is one 36px row** on the Pane's own plane, with no rule under
// it (the body's top padding separates them): dot · title (`W_LABEL`
// `TEXT`) · checkout, the agent tabs, then the right cluster —
// tasks meter · PR/CI · attention jump · head action. Colour is state: the
// checkout, drift and PR are `TEXT_MUTED`; only the CI dot, a failure count
// and the live meter segment carry a hue.
//
// **Below L1** (L2 instruments, the wall) brightness sorts cells: a hot cell
// (working, failing, a Decision, blocked, focused) has a `TEXT_STRONG`
// title, a quiet one `TEXT_2`. Signals are words, never glyph soup, and a
// word is coloured only when it is state.

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
/// The titlebar location's segments: 6px apart, one mono baseline.
pub const TITLE_GAP: f32 = SPACE_1_5;
/// The titlebar's labelled add control: the icon-button face with room for
/// its mono label, the glyph 6px from it.
pub const TITLE_ADD_PAD_X: f32 = SPACE_2;
pub const TITLE_ADD_GAP: f32 = SPACE_1_5;
/// The development-build tag: a quiet mono `dev` in a hairline box, 18px
/// high (it sits inside a 20px UI line), 6px inline padding.
pub const DEV_TAG_H: f32 = 18.0;
pub const DEV_TAG_PAD_X: f32 = SPACE_1_5;
/// The smallest window the chrome still lays out in: the nav plus one Pane
/// at L2, the title, the add control and the Windows caption group.
pub const WINDOW_MIN_W: f32 = 640.0;
pub const WINDOW_MIN_H: f32 = 420.0;

/// 36px — the Pane head: one row, no band. At 36 a 24px head control keeps
/// 6px of air above and below.
pub const PANE_HEAD_H: f32 = 36.0;
/// The head title's cap before it truncates, and the floor it keeps however
/// narrow the head (a shorter title keeps its whole text): the checkout and
/// the agent tabs give way first.
pub const HEAD_TITLE_MAX_W: f32 = 240.0;
pub const HEAD_TITLE_MIN_W: f32 = 96.0;
/// Between the head's dot, title and checkout.
pub const HEAD_GAP: f32 = SPACE_2;
/// Between the head's clusters: title → checkout, and between the facts on
/// the right (tasks · PR/CI · attention · action).
pub const HEAD_CLUSTER_GAP: f32 = SPACE_3;
/// How much more the checkout shrinks than the title when the head is
/// narrow. Drift and dirt give way first, then the branch name — never
/// below a few characters, so a narrow Pane still says where the work is.
pub const HEAD_CHECKOUT_SHRINK: f32 = 4.0;
pub const HEAD_BRANCH_MIN_W: f32 = 64.0;
/// The checkout's floor: its branch mark, the gap and that minimum name.
pub const HEAD_CHECKOUT_MIN_W: f32 = ROW_ICON + ROW_ICON_GAP + HEAD_BRANCH_MIN_W;
/// Between a checkout's directory/branch pairs.
pub const CHECKOUT_GAP: f32 = SPACE_2;
/// The tasks meter in the head: 6 × 3 segments, 1px radius, 2px apart (an
/// 8px pitch). Past `METER_SEG_CAP` steps it is one `METER_TRACK_W` track —
/// the same length as the usage lines, so the two readings share a module.
pub const METER_SEG_W: f32 = 6.0;
pub const METER_SEG_H: f32 = 3.0;
pub const METER_SEG_GAP: f32 = SPACE_0_5;
pub const METER_SEG_R: f32 = 1.0;
pub const METER_SEG_CAP: usize = 12;
pub const METER_TRACK_W: f32 = 48.0;
/// Between the meter and its `3/4` count.
pub const METER_GAP: f32 = SPACE_1_5;
/// The unread ring's brightest breath (its dimmest is `PULSE_MIN`).
pub const UNREAD_PULSE_MAX: f32 = 0.7;
/// The checks card the head's PR/CI chip opens (#29): wide enough for a
/// matrix job's own name — `test (windows-latest, stable)` — beside its
/// state word, which is the whole reason the card exists.
pub const CHECKS_CARD_W: f32 = 312.0;
/// The card's widest: it grows to its tally and run names up to here.
pub const CHECKS_CARD_MAX_W: f32 = 420.0;
pub const CHECKS_CARD_PAD: f32 = SPACE_1;
/// Between the card's heading and its runs.
pub const CHECKS_CARD_GAP: f32 = SPACE_1;
/// The card's heading row and one run's row.
pub const CHECKS_HEAD_H: f32 = 28.0;
pub const CHECKS_ROW_H: f32 = 24.0;
/// A workflow's heading above the runs it owns, and the space that sets
/// that group off from the one before it.
pub const CHECKS_GROUP_H: f32 = 20.0;
pub const CHECKS_GROUP_GAP: f32 = SPACE_1_5;

/// The wall cell: 8px padding, 4px between rows, an 8px status dot — the
/// wall's whole job is the signal, so its dot is bigger than a row's.
pub const WALL_PAD: f32 = SPACE_2;
pub const WALL_ROW_GAP: f32 = SPACE_1;
pub const WALL_DOT: f32 = 8.0;
/// Between a cell's dot and its title (L2 and the wall).
pub const CELL_DOT_GAP: f32 = SPACE_1_5;
/// Between an L2 cell's rows, and between the lines of its tail.
pub const CELL_ROW_GAP: f32 = SPACE_1_5;
pub const CELL_TAIL_GAP: f32 = SPACE_1;
/// A completed L2 cell's history, quieted; the header and the Composer
/// keep full contrast (the header's `done` is the one completion label).
pub const DONE_CELL_OPACITY: f32 = 0.75;

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
/// The empty board's hint column: lines 8px apart, a key 8px from its verb.
pub const EMPTY_BOARD_GAP: f32 = SPACE_2;
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
/// apart: queued prompts (dim `❯` lines), then the one input row — `❯` and
/// the line at left, the model pair and the round send control at right.
/// Under the box, outside it, the meta row (`COMPOSER_META_H`,
/// `COMPOSER_META_GAP` below the box): mode and a draft's setup chips at
/// left, session controls and the usage meter at right, `FS_SM`
/// `TEXT_MUTED`. The Composer writes one key hint, in its placeholder;
/// its controls' tooltips name their keys. Any pad, gap, edge or inset
/// change here must update `pane::composer_fixed_height` in the same commit.
pub const COMPOSER_PAD_X: f32 = BOX_INSET_X - 1.0;
pub const COMPOSER_PAD_T: f32 = SPACE_2;
pub const COMPOSER_PAD_B: f32 = SPACE_2;
pub const COMPOSER_ROW_H: f32 = 20.0;
pub const COMPOSER_GAP: f32 = SPACE_1;
pub const COMPOSER_META_H: f32 = CHIP_H;
pub const COMPOSER_META_GAP: f32 = SPACE_1;
/// The send control: a `COMPOSER_ROW_H` circle, its glyph 10px. It sends
/// (↑) whenever the line has text — queueing behind a running turn — and
/// stops (■) while a turn runs over an empty line.
pub const SEND_BUTTON: f32 = COMPOSER_ROW_H;
pub const SEND_GLYPH: f32 = 10.0;
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
/// The context ring: a 14px box, 5.4px radius, 2px stroke, sweeping
/// clockwise from 12 o'clock with a round cap. No text, ever.
pub const USAGE_RING_D: f32 = 14.0;
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
/// The readout's percent column: room for `100%` at `FS_SM` in tabular
/// figures (four code-cell widths is the generous bound), so 9% → 62% →
/// 100% never shifts the marks beside it.
pub const USAGE_READOUT_W: f32 = 4.0 * FS_SM * CODE_ADVANCE;
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
/// The narrowest draft Pane that still draws the usage meter beside its
/// setup chips and model pair; below it the meter gives way first.
pub const DRAFT_METER_MIN_W: f32 = 560.0;
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
pub const FORM_CHOICE_PAD: f32 = SPACE_1;
/// A choice chip's and a chooser's inline padding inside the 32px row.
pub const FORM_CHIP_PAD_X: f32 = SPACE_2;
pub const FORM_FIELD_PAD_X: f32 = SPACE_2 + SPACE_0_5;
/// A switch row's label block takes at most this share of the row, so a
/// long description wraps before it crowds the switch.
pub const FORM_TEXT_FRACTION: f32 = 0.6;
/// The Settings switch: a 28×16 pill (2px inset), a 12px thumb travelling
/// the pill's inner width.
pub const SWITCH_W: f32 = 28.0;
pub const SWITCH_H: f32 = 16.0;
pub const SWITCH_INSET: f32 = SPACE_0_5;
pub const SWITCH_THUMB: f32 = SWITCH_H - 2.0 * SWITCH_INSET;
pub const SWITCH_TRAVEL: f32 = SWITCH_W - 2.0 * SWITCH_INSET - SWITCH_THUMB;
/// A sheet text button's inline padding (Add Directory, Remove, Done).
pub const FORM_BUTTON_PAD_X: f32 = SPACE_3;
/// A tooltip: mono `FS_SM`, 8px × 4px, at most 280px before it wraps.
pub const TOOLTIP_PAD_X: f32 = SPACE_2;
pub const TOOLTIP_PAD_Y: f32 = SPACE_1;
pub const TOOLTIP_MAX_W: f32 = 280.0;
/// The notifications panel: 340px holds a title, a detail line and an age
/// without wrapping; a row is two lines in 6px of air each side.
pub const NOTICE_PANEL_W: f32 = 340.0;
pub const NOTICE_ROW_H: f32 = LH_UI + LH_META + 2.0 * SPACE_1_5;
/// The bell's unread pill: 14px, 10.5px mono figures, 2px in from the
/// button's corner.
pub const BADGE_H: f32 = 14.0;
pub const FS_BADGE: f32 = 10.5;
pub const BADGE_INSET: f32 = SPACE_0_5;
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
/// A fact row's key column (About): the longest key, "Development build".
pub const FACT_KEY_W: f32 = 136.0;
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
// (end WP-E) — append above this line only

// ======================================== WP-F · decisions and subagents
// Owner: WP-F (decision.rs, subagents.rs, the Decision card and keycaps.)
// Edit values and append tokens only inside this section.

/// The Decision card (approvals, questions, forms, links — Main's and a
/// Subagent's alike): a `RAISED` block under an `ATTENTION_WASH` ground with
/// a 1px `ATTENTION_EDGE` (35%) edge, in the reading column above the
/// Composer. Mono head (`◆` drawn, the kind word `W_LABEL` `ATTENTION`),
/// prose question (Geist `FS_PROSE` `W_STRONG` `TEXT_STRONG`), option rows
/// that each show the one key that picks them, a mono footer. Colour is
/// state: Deny is not red; the card's amber is the only hue it carries.
/// 12px inline and 10px block padding; 8px between sections.
pub const DECISION_PAD_X: f32 = SPACE_3;
pub const DECISION_PAD_Y: f32 = 10.0;
pub const DECISION_GAP: f32 = SPACE_2;
/// The head's drawn diamond: 8px in a `LH_META` line.
pub const DECISION_MARK: f32 = SPACE_2;
/// From the card to the Composer below it, and between stacked cards.
pub const DECISION_DOCK_GAP: f32 = SPACE_2;
/// An option row: 8px inline, 4px block padding around a `LH_PROSE_SM` line
/// (26px with no description), 2px apart; its keycap sits 8px before the
/// label. Hover is `FILL` (the row rests on `RAISED`), selected is `FILL`
/// plus a trailing `ACCENT` check — never a focus-coloured border.
pub const DECISION_ROW_PAD_X: f32 = SPACE_2;
pub const DECISION_ROW_PAD_Y: f32 = SPACE_1;
pub const DECISION_ROW_GAP: f32 = SPACE_0_5;
pub const DECISION_ROW_INNER_GAP: f32 = SPACE_2;
/// The selected row's trailing check.
pub const DECISION_CHECK: f32 = SPACE_3;
/// A question's text to its rows, and one question to the next.
pub const DECISION_QUESTION_GAP: f32 = SPACE_2;
pub const DECISION_QUESTIONS_GAP: f32 = SPACE_4;
/// The command well: `GROUND`, 6/10 padding, scrolling past 160px.
pub const DECISION_WELL_PAD_X: f32 = 10.0;
pub const DECISION_WELL_PAD_Y: f32 = SPACE_1_5;
pub const DECISION_INPUT_MAX_H: f32 = 160.0;
/// A question body's scroll cap inside the card (head and footer stay
/// pinned); container-relative, never a window fraction.
pub const DECISION_BODY_MAX_H: f32 = 320.0;
/// Below a 360px Pane the body caps at two described option rows and
/// scrolls, so the head and the answer row always stay in reach.
pub const DECISION_SHORT_PANE_H: f32 = 360.0;
pub const DECISION_SHORT_BODY_MAX_H: f32 =
    2.0 * (2.0 * LH_PROSE_SM + 2.0 * DECISION_ROW_PAD_Y) + DECISION_ROW_GAP;
/// The scroll gutter a body keeps free for its thumb.
pub const DECISION_SCROLL_GUTTER: f32 = SPACE_1;
/// A question's "type your own answer" field: the sheet field recipe
/// (`PANE`, `INPUT_EDGE`, `R_CONTROL`, `CONTROL_H`, mono `FS_UI`) with 6px
/// inline padding, its edge and padding hanging left of the option labels'
/// column so its text starts on it.
pub const QUESTION_FIELD_PAD_X: f32 = SPACE_1_5;
/// An L2 keycap pair (`y allow`): key, 4px, verb; pairs 12px apart.
pub const DECISION_KEY_GAP: f32 = SPACE_1;
pub const DECISION_KEYS_GAP: f32 = SPACE_3;
/// The L2 Decision body: the cell's padding, 6px between its lines.
pub const DECISION_L2_GAP: f32 = SPACE_1_5;

/// Subagent tabs: Ferrite's own row of 20px tabs packed with no gap, 8px
/// inline padding, no edge and no rule under the row; the active tab a
/// `FILL` pill; labels truncate at 112px. A mark sits 6px after its label;
/// the `+N` overflow keeps 4px either side.
pub const SUBJECT_TAB_PAD_X: f32 = SPACE_2;
pub const SUBJECT_TAB_GAP: f32 = SPACE_1;
pub const SUBJECT_TAB_INNER_GAP: f32 = SPACE_1_5;
pub const SUBJECT_LABEL_MAX_W: f32 = 112.0;
/// The strip's row: the 20px pills with 2px of air above and below.
pub const SUBJECT_STRIP_H: f32 = CHIP_H + 2.0 * SPACE_0_5;
/// A working tab's busy dots: three 2px dots 2px apart (10px, no slack),
/// lifting 2px on a 650ms loop; still under reduced motion.
pub const BUSY_DOT_D: f32 = SPACE_0_5;
pub const BUSY_DOT_GAP: f32 = SPACE_0_5;
pub const BUSY_DOT_LIFT: f32 = SPACE_0_5;
pub const BUSY_DOTS_MS: u64 = 650;
pub const BUSY_DOTS_W: f32 = 3.0 * BUSY_DOT_D + 2.0 * BUSY_DOT_GAP;
/// The one needs-you dot: a waiting tab and the head's jump control.
pub const ATTENTION_DOT: f32 = 5.0;
/// A failed agent's drawn `✗`, in `BLOCKED`, beside its label.
pub const SUBJECT_FAILED_MARK: f32 = SPACE_2;
/// The head's jump control: a `CHIP_H` square around the dot.
pub const ATTENTION_JUMP: f32 = CHIP_H;
// (end WP-F) — append above this line only

// ======================================== WP-G · nav
// Owner: WP-G (nav.rs and its cockpit wiring.)
// Edit values and append tokens only inside this section.
//
// **The nav is one column grid on `GROUND`.** Every row — a Thread, a Group,
// a Project heading, the Parked header, and the filter trigger in the head —
// lays out `ROW_PAD_X | lead slot NAV_LEAD_W | NAV_LEAD_GAP | text … | mark`.
// The lead slot holds the row's one glyph (status dot, Group glyph, folder,
// fold chevron), so every title, label and meta line starts on one x, and
// the head's folder sits on the same axis as the rows' dots (head inset 8 +
// trigger inset 8 = tree inset 8 + row inset 8). The meta line hangs under
// the title at that x, and its tail (subagents · age) ends under the mark.
// Selection is one `FILL` on the focused Thread's row; nothing else fills.

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
/// 12px — a row's lead slot: the status dot, the Group glyph, the folder in
/// the filter trigger and a Project heading, the Parked chevron.
pub const NAV_LEAD_W: f32 = GLYPH_BOX;
/// 6px — from the lead slot to the row's text.
pub const NAV_LEAD_GAP: f32 = SPACE_1_5;
/// 18px — where a row's text starts inside its own padding: the title, the
/// hanging meta line, a heading's label.
pub const NAV_TEXT_X: f32 = NAV_LEAD_W + NAV_LEAD_GAP;
/// 8px — from a title to the provider mark at the row's right.
pub const NAV_MARK_GAP: f32 = SPACE_2;
/// 4px — between the facts at the tail of a meta line (subagents, age) and
/// between a meta fact and its `·` seam.
pub const NAV_TAIL_GAP: f32 = SPACE_1;
/// 254px — the content box of a root-level nav row: the column less the
/// tree's inline padding, less the row's own. A truncating title has to be
/// pinned to it, because gpui only measures an ellipsis against a width it
/// knows on the line's very first measure (see `nav::group_row`).
pub const ROW_TEXT_W: f32 = NAV_WIDTH - 2.0 * NAV_TREE_PAD - 2.0 * ROW_PAD_X;
/// 28px — a one-line row: a Thread in Project order, or a parked row in
/// that order. The same padding as a two-line row around one title line.
pub const NAV_COMPACT_ROW_H: f32 = 2.0 * ROW_PAD_Y + LH_TIGHT;
/// 28px — a section heading's row (a Project heading, the Parked header):
/// one metadata line in the rows' own padding.
pub const NAV_SECTION_H: f32 = 2.0 * ROW_PAD_Y + LH_META;
/// 16px — every block boundary in the tree: between two Group blocks (a
/// drop band as well as air), above a run of solos after a Group, above a
/// Project heading. The 44px rows carry their own air, so one step is
/// enough to say "new block".
pub const GROUP_GAP: f32 = SPACE_4;
pub const SOLOS_TOP: f32 = GROUP_GAP;
/// 6px between a Group row and its members; 2px between sibling rows.
pub const MEMBERS_TOP: f32 = SPACE_1_5;
pub const MEMBER_GAP: f32 = SPACE_0_5;
/// 18px — the member indent: a member's lead slot starts under its Group's
/// title, the tree grammar of a child's marker under its parent's text.
pub const MEMBER_INDENT: f32 = NAV_TEXT_X;
/// The 1px rail hangs from the Group glyph's centre: `RAIL_OFFSET` left of
/// the members box, inset 3px top and bottom. Translucent — draw it with
/// `rgba`.
pub const RAIL_OFFSET: f32 = MEMBER_INDENT - ROW_PAD_X - NAV_LEAD_W / 2.0;
pub const RAIL_INSET: f32 = 3.0;
pub const NAV_GROUP_RAIL: u32 = HAIRLINE_STRONG;
/// A nav row's radius: the block radius, so a selected row reads as a soft
/// card on the ground rather than a control.
pub const NAV_ROW_R: f32 = R_BLOCK;
/// 12px — above a section heading (a Project, the Parked fold).
pub const NAV_SECTION_GAP: f32 = SPACE_3;
/// 12px — the provider logomark in a nav row: the lead slot's size, so the
/// row's two glyph columns match.
pub const PROVIDER_MARK: f32 = GLYPH_BOX;
/// A failing Thread's halo: it is still inferring, so it still breathes,
/// in the failure's ink (`BLOCKED` at the running halo's 35%).
pub const NAV_FAILING_HALO: u32 = 0xd2908959;
/// How far a rail item's status dot sits in from its box's corner.
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
pub const NAV_RAIL_ITEM_GAP: f32 = if cfg!(target_os = "macos") {
    SPACE_1
} else {
    MEMBER_GAP
};
pub const NAV_RAIL_ITEMS_TOP: f32 = SPACE_3;
/// The most of the column the open Parked section may take. Its list
/// scrolls past this, so a hundred parked Threads never push the running
/// tree out of sight.
pub const NAV_PARKED_MAX_SHARE: f32 = 0.5;
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
            &[(
                "TEXT_SELECTION_WASH on PANE",
                over(TEXT_SELECTION_WASH, PANE),
            )],
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
        assert!(near(CODE_CELL, 7.5));
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
            assert_eq!(tokens.tab_active.color, solid(FILL));
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
            assert_eq!(theme.mono_font_family.as_ref(), FONT_CODE);
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
    const GEIST: &[u8] = include_bytes!("../assets/fonts/Geist.ttf");
    /// Both bundled faces, by name: a glyph text may use is in each.
    const FACES: [(&str, &[u8]); 2] = [("Geist", GEIST), ("Geist Mono", GEIST_MONO)];

    #[test]
    fn faces_are_the_bundled_families() {
        assert_ne!(FONT_UI, FONT_CODE, "UI and code are two faces");
        for face in crate::FONTS {
            let family = family(face);
            assert!(
                family == FONT_UI || family == FONT_CODE,
                "a bundled face names the family `{family}`"
            );
        }
        assert_eq!(family(GEIST), FONT_UI);
        assert_eq!(family(GEIST_MONO), FONT_CODE);
    }

    #[test]
    fn chrome_glyphs_are_in_both_faces() {
        for (name, face) in FACES {
            assert!(covers(face, 'a') && covers(face, '$'), "{name}");
            // The glyphs the grammar must draw as SVG, because a face lacks
            // them.
            for missing in ['❯', '⎿', '∴', '✻', '✓', '✗', '☐'] {
                assert!(!covers(face, missing), "{missing} is covered in {name} now");
            }
            for glyph in CHROME_GLYPHS {
                assert!(covers(face, *glyph), "{glyph} is not in {name}");
            }
        }
        for glyph in KEY_GLYPHS {
            assert!(covers(GEIST_MONO, *glyph), "{glyph} is not in Geist Mono");
        }
    }

    /// Every non-ASCII glyph render code puts in a literal must be one both
    /// bundled faces draw: `❯ ⎿ ∴ ✻ ✓ ✗ ☐ ◆ ⌘` and friends are SVG glyph
    /// boxes or painted marks. Scans the render modules' non-test source,
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
                    // A key's glyph is drawn in the code face only.
                    let faces: &[(&str, &[u8])] = if KEY_GLYPHS.contains(&c) {
                        &FACES[1..]
                    } else {
                        &FACES
                    };
                    for (name, face) in faces {
                        if !covers(face, c) {
                            missing.push(format!(
                                "{file}:{} {c} (U+{:04X}) not in {name}",
                                at + 1,
                                c as u32
                            ));
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "glyphs a bundled face lacks:\n{}",
            missing.join("\n")
        );
    }
}
