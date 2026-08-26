//! The Aperture visual system — every token the comps define, named once.
//!
//! Values are transcribed from the design canon on issue #22 (§1 of the
//! Concept-page spec, extracted verbatim from the canon artboards) with the
//! role names of the sidebar/impl spec, also on #22. Render code (`pane.rs`,
//! `cockpit.rs`, `composer.rs`, `nav.rs`) imports from here and holds no
//! color literal of its own; core stays color-blind.
//!
//! Solid colors are `0xRRGGBB` and drawn with `gpui::rgb`; translucent ones
//! are `0xRRGGBBAA` and drawn with `gpui::rgba`. The alpha byte is the
//! comp's fraction × 255, rounded.

// ---------------------------------------------------------------- grounds

/// `#050505` — the window behind everything.
pub const GROUND: u32 = 0x050505;
/// `#0e0e0e` — Pane/cell surface.
pub const SURFACE: u32 = 0x0e0e0e;
/// `#0a0a0a` — one step down: code-block interiors, sunken strips.
pub const INSET: u32 = 0x0a0a0a;
/// `#191919` — chips, keycaps, menus, inline-code pills.
pub const RAISED: u32 = 0x191919;
/// `#161616` — the Composer's own ground.
pub const COMPOSER: u32 = 0x161616;

// ------------------------------------------------------------------ lines

/// `rgba(255,255,255,0.045)` — internal hairline dividers.
pub const HAIRLINE: u32 = 0xffffff0b;
/// `rgba(255,255,255,0.05)` — the popover's faint outer ring, the first
/// layer of the popover elevation recipe (the root selector, #24; #23's
/// slash / @ menus will share it).
pub const RING_FAINT: u32 = 0xffffff0d;
/// `rgba(255,255,255,0.07)` — Pane/card borders AND the progress-bar track.
pub const EDGE: u32 = 0xffffff12;
/// `rgba(255,255,255,0.12)` — strong borders: Composer top, keycaps,
/// markdown-heading underline.
pub const EDGE_STRONG: u32 = 0xffffff1f;

// -------------------------------------------------------------------- ink

/// `#f3f4f7` — titles, prompts, typed text, bold tool names.
pub const INK: u32 = 0xf3f4f7;
/// `#a7abb4` — agent prose, dimmed titles, diff-chip base ink.
pub const INK_SECONDARY: u32 = 0xa7abb4;
/// `#8b8f97` — ⏺ gutter glyph, activity lines, captions.
pub const INK_TERTIARY: u32 = 0x8b8f97;
/// `#7f8187` — labels, hints, meta, idle text, ⎿ continuations.
pub const INK_MUTED: u32 = 0x7f8187;
/// `#54575f` — faintest: unchanged line numbers, `·` separators, comments.
pub const INK_FAINT: u32 = 0x54575f;

// -------------------------------------------------------- accent + signals

/// `#c7ccd6` — THE accent: ❯ glyph, links, cursor, progress fill, focus ring.
pub const ACCENT: u32 = 0xc7ccd6;
/// `#d7dbe3` — link hover, the only hover state the comps draw. Reserved:
/// v1 renders paths inert (sidebar-and-impl §4.3 — real links are a later
/// pointer slice).
#[allow(dead_code)]
pub const ACCENT_HOVER: u32 = 0xd7dbe3;
/// `rgba(199,204,214,0.14)` — accent chip ground: mode chip, @-mention
/// pill (both arrive with #23's Composer behaviors); today it grounds the
/// selection wash (`SELECTION`).
pub const ACCENT_WASH: u32 = 0xc7ccd624;
/// `rgba(199,204,214,0.40)` — link underline color. Reserved with
/// `ACCENT_HOVER`.
#[allow(dead_code)]
pub const ACCENT_UNDERLINE: u32 = 0xc7ccd666;
/// `#7fc99b` — green: running LED, pass badge, diff `+`.
pub const GOOD: u32 = 0x7fc99b;
/// `rgba(127,201,155,0.13)` — pass-badge ground, diff added-row ground.
pub const GOOD_WASH: u32 = 0x7fc99b21;
/// `rgba(127,201,155,0.30)` — intra-line diff emphasis. Reserved for v2:
/// word-level diffing is a second diff engine (sidebar-and-impl §4.3).
#[allow(dead_code)]
pub const GOOD_STRONG: u32 = 0x7fc99b4d;
/// `#d8c082` — amber = Decision/attention; doubles as syntax number/function.
pub const WAIT: u32 = 0xd8c082;
/// `rgba(216,192,130,0.13)` — Decision-card ground, needs-you chip ground.
pub const WAIT_WASH: u32 = 0xd8c08221;
/// `rgba(216,192,130,0.35)` — Decision-card border.
pub const WAIT_EDGE: u32 = 0xd8c08259;
/// `#e08c84` — red: fail badge, blocked LED/ring, diff `−`.
pub const FAIL: u32 = 0xe08c84;
/// `rgba(224,140,132,0.13)` — fail-badge ground, diff removed-row ground.
pub const FAIL_WASH: u32 = 0xe08c8421;
/// The idle/parked LED — the tertiary ink in LED role. An alias, not a
/// fresh value: the comps reuse `#8b8f97` for both, and the signal keeps
/// its own name so a future retune can split them without a rename.
pub const IDLE: u32 = INK_TERTIARY;

// ---------------------------------------------------------------- shadows

/// `rgba(0,0,0,0.30)` — the `0 2px 4px` elevation layer. Panes and Cockpit
/// cells stay border-only (the Main board's floating-pane shadow is open
/// question concept.md §9.15); popovers carry it (#24's root selector).
pub const SHADOW_NEAR: u32 = 0x0000004d;
/// `rgba(0,0,0,0.40)` — the `0 6px 16px -4px` elevation layer. Same status.
pub const SHADOW_FAR: u32 = 0x00000066;

// ----------------------------------------------------------------- syntax

/// `#8fb3d9` — code keywords. `Class::Plain → ACCENT`, `Str → CODE_STR`,
/// `Number → WAIT`, `Comment → INK_FAINT` per the sidebar map's class table.
pub const CODE_KEYWORD: u32 = 0x8fb3d9;
/// `#9ec78a` — code strings.
pub const CODE_STR: u32 = 0x9ec78a;

// ------------------------------------------------------- app-only mappings

/// The transcript/Composer selection wash. No comp draws selection, so it
/// borrows the accent chip wash — the only accent tint in the system —
/// instead of keeping the shipped blue (`#3f6ea830`; no blue survives the
/// retune).
pub const SELECTION: u32 = ACCENT_WASH;
/// Row hover for rows with no ground of their own — the comps draw hover on
/// links only (concept.md §9.7), so this stays at the hairline tone the nav
/// already uses rather than inventing a stronger one.
pub const HOVER: u32 = HAIRLINE;
/// Fully transparent — the reserved slot a focus bar or ring occupies when
/// off, so nothing shifts when it turns on.
pub const TRANSPARENT: u32 = 0x00000000;

// ------------------------------------------------------------------- type

/// 9px — wall-cell status line.
pub const TEXT_WALL_STATUS: f32 = 9.0;
/// 9.5px — cell-header meta, small chips, legend items.
pub const TEXT_CHIP_SM: f32 = 9.5;
/// 10px — hints, badges, keycaps, activity lines, wall-cell names.
pub const TEXT_CHIP: f32 = 10.0;
/// 10.5px — chips, tool-row meta, idle-cell text, progress fraction.
pub const TEXT_META: f32 = 10.5;
/// 11px — Dense header row, wall-header counts, queued-row body (11.5 in
/// the comp; the scale's step below the title).
pub const TEXT_ROW: f32 = 11.0;
/// 11.5px — cell titles (weight 600), queued row.
pub const TEXT_TITLE: f32 = 11.5;
/// 12px — tool rows, diff lines, code, permission command, wall-header title.
pub const TEXT_CODE: f32 = 12.0;
/// 12.5px — the Dense transcript base (the terminal metric).
pub const TEXT_BODY: f32 = 12.5;
/// 13px — Composer input.
pub const TEXT_INPUT: f32 = 13.0;
/// 13.5px — Dense markdown section heading (weight 700).
pub const TEXT_HEADING: f32 = 13.5;

/// 1.45 — Dense transcript line height (canonical).
pub const LINE_TRANSCRIPT: f32 = 1.45;
/// 1.5 — code-block line height.
pub const LINE_CODE: f32 = 1.5;

// --------------------------------------------------------------- geometry

/// Grid gap and padding. The Wall board's numbers (gap 6, padding 8) serve
/// every level: grid chrome must not depend on the Level it is laying out,
/// or `cell()` → Level → gap would be circular. (The Cockpit board's L2
/// grid draws gap 8 / padding 10 — a 2px deviation accepted for totality.)
pub const GRID_GAP: f32 = 6.0;
pub const GRID_PAD: f32 = 8.0;
/// 34px — the Cockpit strip (wall header).
pub const STRIP_H: f32 = 34.0;
/// 30px — the wall's pinned legend.
pub const LEGEND_H: f32 = 30.0;
/// 28px — the Dense single-row Pane header.
pub const HEADER_DENSE_H: f32 = 28.0;
/// 24px — Cockpit cell header; queued row.
pub const CELL_HEADER_H: f32 = 24.0;
/// 40px — Composer input row (min-height; the line grows).
pub const COMPOSER_H: f32 = 40.0;
/// 22px — Composer meta row.
pub const COMPOSER_META_H: f32 = 22.0;
/// 26px — popover menu rows (the comps' slash/@ menus; the root selector's
/// rows per issue #24's pinned design).
pub const MENU_ROW_H: f32 = 26.0;
/// 22px — the popover's key-hint footer, the same scale step as the
/// Composer meta row.
pub const POPOVER_FOOTER_H: f32 = 22.0;
/// 4px — the popover's own padding around its rows.
pub const POPOVER_PAD: f32 = 4.0;
/// 260px — the root-selector popover's width (issue #24's pinned design;
/// the comps draw their popovers at the composer's width, which a header
/// anchor does not have).
pub const POPOVER_W: f32 = 260.0;
/// 14px — the Composer stack's horizontal padding (input, queued, meta).
pub const COMPOSER_PAD_X: f32 = 14.0;
/// 6px LED dot (Dense header, Cockpit cells) / 5px on the wall.
pub const LED: f32 = 6.0;
pub const LED_WALL: f32 = 5.0;
/// 6px progress pill (L2/L1) / 5px on the wall; track `EDGE`, fill `ACCENT`.
pub const BAR_H: f32 = 6.0;
pub const BAR_H_WALL: f32 = 5.0;
/// Radii: 3 inline code/code blocks · 4 chips/keycaps/badges · 5 cards.
pub const R_TIGHT: f32 = 3.0;
pub const R_CHIP: f32 = 4.0;
pub const R_CARD: f32 = 5.0;
/// 1.5px — the inset focus/Decision/blocker ring.
pub const RING_W: f32 = 1.5;
/// 14px — the transcript glyph gutter (❯ / ⏺ / list numbers).
pub const GUTTER_W: f32 = 14.0;
/// 22px — indent under a tool row (⎿ continuations, bare diffs, markdown).
pub const INDENT: f32 = 22.0;
/// 30px — the bare diff's right-aligned line-number column.
pub const DIFF_NUM_W: f32 = 30.0;
/// 7px — the block cursor's width (7×16 at 13px rows).
pub const CURSOR_W: f32 = 7.0;
/// Transcript padding, Dense: 8px vertical, 12px horizontal; 4px row gap.
pub const TRANSCRIPT_PAD_X: f32 = 12.0;
pub const TRANSCRIPT_PAD_Y: f32 = 8.0;
pub const TRANSCRIPT_GAP: f32 = 4.0;
/// Cell body padding (Cockpit cells).
pub const CELL_PAD: f32 = 10.0;

/// A done Cockpit cell's whole-cell opacity (concept.md §1.8).
pub const DONE_CELL_OPACITY: f32 = 0.75;
/// A done wall cell's dimming (glance.md §1.3).
pub const DONE_WALL_OPACITY: f32 = 0.6;

// ------------------------------------------------------------------ faces

/// The mono face. The comps' JetBrains Mono is not bundled in v1
/// (issue #22 AC): Menlo ships with macOS, Consolas with Windows
/// (Cascadia Mono only arrives with Windows Terminal, so it cannot be
/// assumed).
#[cfg(target_os = "macos")]
pub const FONT_MONO: &str = "Menlo";
#[cfg(not(target_os = "macos"))]
pub const FONT_MONO: &str = "Consolas";

/// The UI face for chrome (titles, cell names): the platform's system UI
/// font, which gpui resolves from this special name on both platforms —
/// the comps' Geist is not bundled in v1.
pub const FONT_UI: &str = ".SystemUIFont";
