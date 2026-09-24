//! Ferrite's visual system: every colour, face, size and metric, named once.
//! This module doc is where the design rules live; there is no other design
//! document. Render code imports from here and holds no colour or metric
//! literal of its own; core stays colour-blind.
//!
//! **Terminal grammar, application craft.** Ferrite keeps the provider CLIs'
//! vocabulary (`❯` prompts, tool bullets, result elbows, a monospace voice for
//! what a machine printed) and renders it cleanly, in Geist chrome. No chat bubbles, no avatars,
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
//!    Green never means "finished". The one exception is brand, not state:
//!    a provider's logomark wears its own colour (`PROVIDER_*`) wherever it
//!    appears — nav rows, the model chip, picker rows — and nothing else.
//!    **Hues can be counted:** at rest the only non-grey pixels are status
//!    dots, provider marks, links and a failed word; any other hue means
//!    something needs you. **Colour sits on the word**, not the row, ground
//!    or ring: a state shows once per surface (one alpha edge, one dot, or
//!    one word or lead phrase), and no state tints a ground (the ochre
//!    wash is retired; rich text never uses `ACCENT_WASH`; inline code is
//!    `INLINE_CODE_INK` = `TEXT`; a waiting Pane's edge is
//!    `ATTENTION_EDGE` at alpha).
//! 3. **Opaque faces, alpha edges.** Planes and hover/fill faces are opaque
//!    `rgb()` values (a hover must never be tinted by what lies under it, see
//!    `pointer.rs`). Hairlines, washes, rings over content and veils are alpha
//!    `rgba()` values.
//! 4. **An ordered elevation ladder.** `GROUND` (window, nav, board) <
//!    `PANE` < `RAISED` (Composer, code, cards, menus) < `RAISED_2` (keycaps,
//!    chips on a raised block) < `FILL` (selected) < `FILL_HOVER`. `HOVER` is
//!    the hover face on `GROUND`/`PANE` only, because it would be invisible
//!    on `RAISED`. **The ladder on `RAISED`:** rest `RAISED`; hover
//!    `HOVER_RAISED` (`RAISED_2`); the cursor or a selected row `FILL`; that
//!    row under the pointer `FILL_HOVER`; press `FILL_HOVER`. The pointer
//!    half blends over 150ms (`pointer.rs`); the cursor's `FILL` and a press
//!    land at once. The Stop control's ground is never `TEXT_STRONG`
//!    (`SEND_STOP_GROUND`): only an armed Send is bright. Floating surfaces are
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
//!    call line and every line of its output — the whole call line is one
//!    CLI token, `Bash(cargo test -p ferrite nav::)`, set in Geist Mono at
//!    400: the name in `TEXT`, the parens and arguments in `TEXT_MUTED`, on
//!    the shared line box, at every tier (the operator's ruling, Q1) —
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
//! 11. **Words.** Ferrite's own copy speaks one shared word list
//!     (`theme::words`, beside the state inks and tested against the
//!     notifications, the Decision card and the transcript): `needs you`,
//!     `failing N`, `failed`, `interrupted`, `working`, `done`. Titles,
//!     buttons, menu items, labels and empty states are sentence case
//!     (`New thread`, `Delete thread`); state and value tokens are always
//!     lowercase, even leading a row; Title Case lives only in the macOS menu
//!     bar. `·` (in `TEXT_FAINT`) is the only separator inside a line — no
//!     colon labels, no em dash, no final period on one-line copy — and no
//!     surface prints `now`. A shortcut in a tooltip is a mono `TEXT_MUTED`
//!     suffix with glyph modifiers (`Toggle sidebar ⌘B`), read from the
//!     keymap, never typed by hand.
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
/// On `RAISED` the hover face is `HOVER_RAISED`.
pub const HOVER: u32 = 0x1d2024;
/// `#21252a` — one step above `RAISED`: keycaps, chips on a raised block.
pub const RAISED_2: u32 = 0x21252a;
/// The hover face of anything on `RAISED` (menu rows, options, keycaps):
/// `RAISED_2`, one step up, and one step under the cursor's `FILL`, so a
/// hovered row never reads as the armed one (rule 4).
pub const HOVER_RAISED: u32 = RAISED_2;
/// `#24282e` — the selected fill (the focused Thread's row, an active tab,
/// the menu cursor, a selected option).
pub const FILL: u32 = 0x24282e;
/// `#2b2f36` — a filled row under the pointer.
pub const FILL_HOVER: u32 = 0x2b2f36;
/// The pressed shade: one step past `FILL`.
pub const PRESSED: u32 = FILL_HOVER;

// ------------------------------------------------------------------ edges

/// `#ffffff14` (8%) — the one rule weight: a Pane's resting edge, the rule
/// under the Pane head, a Markdown thematic break, separators.
pub const HAIRLINE: u32 = 0xffffff14;
/// `#ffffff24` (14%) — the stronger rule: floating edges (menu, popover,
/// tooltip, toast), the Composer's resting edge, a blockquote's rule, the
/// rule under a table's header.
pub const HAIRLINE_STRONG: u32 = 0xffffff24;
/// The Composer's resting edge.
pub const COMPOSER_EDGE: u32 = HAIRLINE_STRONG;
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
/// `#afbaca24` (14%) — the accent as a ground: a selected accent row, the
/// slot a dragged Pane would take. Never inline code (rule 6 of the accent:
/// inline code and chips are neutral, `INLINE_CODE_WASH`).
pub const ACCENT_WASH: u32 = 0xafbaca24;
/// `#afbaca40` (25%) — native text selection, painted over glyphs.
pub const TEXT_SELECTION_WASH: u32 = 0xafbaca40;
/// The caret.
pub const CARET: u32 = ACCENT;

// -------------------------------------------------------- state + signals

/// `#8cb59d` — live work (a sage, 22%): the running status dot, a running signal line, the
/// pass chip, diff `+`.
pub const RUNNING: u32 = 0x8cb59d;
/// `#cbb280` — a Decision (a muted ochre, 42%): the status dot, the signal line, the Pane's edge,
/// the Decision card's mark.
pub const ATTENTION: u32 = 0xcbb280;
/// A waiting Pane's edge on a board (C6): ochre at 35%, so the one
/// answer-target cell at full `ATTENTION` stands out, and focus beside it
/// stays readable.
pub const ATTENTION_EDGE: u32 = 0xcbb28059;
/// `#d29089` — blocked or failed (a dusty red, 45%): the status dot, the signal line, the
/// Pane's edge, diff `−`, the word "failed".
pub const BLOCKED: u32 = 0xd29089;
/// A closed Pane's edge on a board: `BLOCKED` at 35%.
pub const BLOCKED_EDGE: u32 = 0xd2908959;
/// Blocked as a one-line ground (a refused drop, a destructive control
/// under the pointer); never a multi-line wash (`DIFF_REMOVED_WASH`).
pub const BLOCKED_WASH: u32 = 0xd290891f;
/// The idle/parked status dot: the muted ink in a dot role.
pub const IDLE: u32 = TEXT_MUTED;

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
/// `TEXT_MUTED` — colour always means something needs you.
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
/// green. Only the mark wears it: never a label, a row or a state.
pub const PROVIDER_CODEX: u32 = 0x10a37f;
pub const PROVIDER_CLAUDE: u32 = 0xd97757;

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
//
// **The type scale.** Four sizes, each one role, each paired with one pixel
// line height (rule 7). Render sites name a role, never a number:
//
// | role          | size | line | face          | used for                              |
// |---------------|------|------|---------------|---------------------------------------|
// | `FS_PROSE`    | 14   | 22   | UI            | agent prose, the Decision question    |
// | `FS_UI`       | 12.5 | 20   | UI or code    | every chrome line: rows, titles,      |
// |               |      |      |               | labels, values, buttons, menu rows    |
// | `FS_PROSE_SM` | 12.5 | 18   | UI            | a description that wraps (Settings,   |
// |               |      |      |               | sheets, option descriptions)          |
// | `FS_SM`       | 11.5 | 16   | UI or code    | metadata: details, hints, section     |
// |               |      |      |               | titles, keycaps, chips, ages, counts, |
// |               |      |      |               | badges, tooltips                      |
//
// Headings step up from the prose size (`heading_scale`: 18 · 16 · 14) and
// only inside the transcript. No surface outside it — menus, popovers,
// sheets, cards, notifications, toasts, tooltips, the titlebar, empty
// states — uses any size but `FS_UI` and `FS_SM`, plus `FS_PROSE_SM` for a
// sheet's wrapping descriptions. A surface's hierarchy comes from ink and
// weight: its one title `W_LABEL`, section titles `FS_SM` `W_LABEL`
// `TEXT_MUTED`, rows `FS_UI` `W_BODY`, details `FS_SM` `W_BODY`.
//
// **Weights:** `W_BODY` (400) for everything that is read — rows, buttons,
// armed menu rows, all mono. W_LABEL once per heading role; weight never
// signals state on a row that can truncate (unread, selected and armed
// change ink or fill, never weight). `W_STRONG` (600) only where prose says
// so (headings, `**strong**`, the Decision question). 700 is never used,
// and no render site names a `FontWeight` itself.
//
// **Figures:** any number that changes while it is on screen — counts,
// percentages, ages, durations, costs, tallies — is `components::tabular`,
// so a tick never moves its neighbours.

/// 14px — agent prose (Geist), the size an operator reads at length; also the
/// Decision question and option descriptions. Paired with `LH_PROSE`.
pub const FS_PROSE: f32 = 14.0;
/// 12.5px — the UI size: every chrome line (menu rows, nav and Pane titles,
/// Settings labels, buttons) in Geist, and code in Geist Mono (prompts, tool
/// output, the Composer). Paired with `LH_UI` (single-line rows) or
/// `LH_CODE` (multi-line mono blocks).
pub const FS_UI: f32 = 12.5;
/// 12.5px — secondary prose (Geist): option labels and descriptions, notes,
/// a Settings row's description. Prose is never smaller. Paired with
/// `LH_PROSE_SM`.
pub const FS_PROSE_SM: f32 = 12.5;
/// 11.5px — metadata: checkout lines, durations, hints, keycaps, chips,
/// timestamps, section titles, badges, tooltips. The floor: nothing is
/// smaller. Paired with `LH_META`.
pub const FS_SM: f32 = 11.5;

/// 22px — prose.
pub const LH_PROSE: f32 = 22.0;
/// 18px — secondary prose.
pub const LH_PROSE_SM: f32 = 18.0;
/// 20px — single-line UI and mono rows.
pub const LH_UI: f32 = 20.0;
/// 18px — multi-line mono blocks: code, diffs, tool output.
pub const LH_CODE: f32 = 18.0;
/// 16px — metadata at `FS_SM`.
pub const LH_META: f32 = 16.0;

/// Weights (see the type scale above): 400 body; 500 a surface's single
/// title, section titles and labels; 600 prose headings, `**strong**` and
/// the Decision question. 700 is not used.
pub const W_BODY: FontWeight = FontWeight::NORMAL;
pub const W_LABEL: FontWeight = FontWeight::MEDIUM;
pub const W_STRONG: FontWeight = FontWeight::SEMIBOLD;

/// The transcript's reading size (cmd-= / cmd-- / cmd-0, every Pane)
/// scales prose, not execution or chrome: the answer size is the setting's
/// own px.
pub fn answer_text_size(size: ferrite_core::settings::ReadingSize) -> f32 {
    f32::from(size.px())
}

/// The pixel line height paired with each reading size, about 1.55× and
/// whole: 12/19 · 13/20 · 14/22 · 15/23 · 16/24 · 18/28 · 20/31 · 22/34 ·
/// 24/37.
pub fn answer_line_height(size: ferrite_core::settings::ReadingSize) -> f32 {
    match size.px() {
        12 => 19.,
        13 => 20.,
        14 => LH_PROSE,
        15 => 23.,
        16 => 24.,
        18 => 28.,
        20 => 31.,
        22 => 34.,
        24 => 37.,
        px => (f32::from(px) * 1.55).round(),
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
//
// **Radii say role**, and a nested surface is **concentric** with the one
// it sits in: `inner = outer − inset`, where the inset is the outer
// surface's padding. Its 1px hairline is not counted — at 1px the arcs
// still read as one family — except for a child painted flush to the
// edge, which takes the exact padding-box radius `outer − 1`. Every
// nesting in the app lands on a token:
//
// | outer                         | inset            | inner                    |
// |-------------------------------|------------------|--------------------------|
// | floating surface `R_BLOCK` 8  | `FLOAT_PAD` 4    | menu row `R_MENU_ROW` 4  |
// | checks card `R_BLOCK` 8       | `FLOAT_PAD` 4    | run row `R_CHIP` 4       |
// | segmented tray `R_CONTROL` 6  | `FORM_CHOICE_PAD` 2 | choice chip `R_CHIP` 4 |
//
// Where the inset is at least the outer radius (a field 16px inside a
// sheet, a control in a setting card's row, a chip in a card's row, a
// keycap in a Decision option) the inner
// corner no longer shares the outer arc, and the inner surface takes its
// own role radius. The one exception is the kit's popup menu (the choice
// menus, the Settings chooser): it rounds its surface and its rows alike
// from the kit's single `radius` (`R_CONTROL`), and its wrapper follows the
// surface so the float shadow hugs it.

/// 10px — a Pane, and a sheet (Settings, the Project editor).
pub const R_PANE: f32 = 10.0;
/// 8px — blocks: the Composer, code, cards, menus, popovers, toasts. The
/// kit's `radius_lg`.
pub const R_BLOCK: f32 = 8.0;
/// 6px — controls: buttons, fields, nav rows, pickers, a segmented tray,
/// tooltips. The kit's `radius`.
pub const R_CONTROL: f32 = 6.0;
/// 4px — chips, keycaps, inline code, and anything nested one inset inside
/// a larger radius: a menu's rows (`R_BLOCK` less `FLOAT_PAD`), a segmented
/// choice's chips (`R_CONTROL` less `FORM_CHOICE_PAD`).
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
/// strip (location, need-you count, `dev`, add control; on Windows the caption buttons,
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
/// drawn inside the shell's `overflow_hidden()` would be clipped. Every
/// control's keyboard focus is the same 1px of the same ink, inset
/// (`components::control_focus`); only the primary button's outline is
/// `TEXT_STRONG`, on its steel face.
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
/// A menu section title row: 24px, its `FS_SM` title sat on the row's foot
/// so it hugs the rows it heads.
pub const MENU_SECTION_H: f32 = 24.0;
/// 10px — a section title's mark: the `FS_SM` cap band (`KEY_GLYPH`), so a
/// provider mark beside an 11.5px title is no heavier than its letters.
pub const MENU_SECTION_ICON: f32 = KEY_GLYPH;
/// 8px — what splits one group of rows from the next inside any floating
/// surface (context-menu groups, a section after rows, a key-hint footer, a
/// panel's head). Space, never a rule: rows sit flush (their 8px of air is
/// inside the 28px row), so a group gap doubles the air between two lines
/// of text — the 2× that makes the split read (`components::menu_separator`).
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
/// and below, the same box as a menu row (`MENU_ROW_H`). Every nav row is
/// this one line. Derived from the type, never summed by hand.
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
/// 20px — one queued prompt's row pitch in the Composer's queue viewport:
/// the input line's own row, stacked with no gap, one `COMPOSER_GAP` above
/// the input.
pub const QUEUE_ROW_H: f32 = COMPOSER_ROW_H;

// ------------------------------------------------------- status and motion

/// 6px — the status dot: the Pane head, nav rows, the wall.
pub const STATUS_DOT: f32 = 6.0;
/// The dimmest a breath goes (an unread dot's own opacity):
/// never all the way out, so it never flickers off.
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
    // The reader's line numbers sit on the Pane itself, not a raised gutter.
    // The gutter is painted opaque over the text, so it takes the Pane's own
    // colour rather than none.
    let mut highlight = (*theme.highlight_theme).clone();
    highlight.style.editor_gutter_background = Some(rgb(PANE).into());
    theme.highlight_theme = std::sync::Arc::new(highlight);
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
    // The selected Settings page is the carried `FILL` row.
    theme.sidebar_accent = rgb(FILL).into();
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
// - **One left edge, three gutter marks.** The gutter holds `❯` (you
//   spoke), a `TOOL_DOT` (a machine action and its state; a group's dot is
//   its worst member's) and the Ferrite mark (the agent spoke). The mark is
//   drawn once per speaker change to the agent, at every tier: on the first
//   prose after a prompt or after a tool or group row. Prose that follows
//   prose — across reasoning, a notice or a record, which neither speak nor
//   hand the floor back — wears none; its gutter stays empty and its text
//   keeps the C1 edge (`transcript::AnswerMarks`, the operator's Q2). A
//   disclosure is a trailing `ICON_CHEVRON` after its row's label, its box
//   always reserved, shown under the pointer or on the keyboard target and
//   turned a quarter when open. No transcript row has a hover ground; the
//   keyboard target alone wears `HOVER`. Nothing sits at the reading
//   column's right but a call's trail (`applied · +N −M`, then its time,
//   tabular; a live call's time ticks whole seconds at 1Hz and freezes when
//   it settles).
// - **State lives in the dot.** A tool's name is neutral ink whatever
//   happened; its dot says how it went (`tool_dot`, static even while
//   live), and a failure colours the one word that says so. A collapsed
//   group is one `TEXT_MUTED` line with tabular figures whose only state ink
//   is ` · N failed`.
// - **One failure line.** A failed call and a failed or interrupted turn
//   read `⎿ failed · 0.1s · <excerpt>`: the lowercase lead in its state ink
//   (`BLOCKED`, or `TEXT_2` for `interrupted`), `·` in `TEXT_FAINT`, the
//   duration `TEXT_MUTED`, and the excerpt the machine printed in the code
//   face, soft-wrapped, never cut.
// - **Machine text is never cut.** Diff lines soft-wrap inside their row
//   (the number and sign on the first line, the wash under every line);
//   only `HUNK_MAX_ROWS` and `OUTPUT_MAX_LINES` hide rows, and they say how
//   many.
// - **Rhythm in three steps**, each at least twice the one inside it
//   (grouping by space, not lines). A turn opens `GAP_TURN` (32) under the
//   one before it; the blocks inside a turn — a prose answer, a group
//   summary, a lone tool row, the stamp — sit `GAP_BLOCK` (12, the block
//   step) apart; the rows of one run of work sit `GAP_ROW` (4, the row
//   step) apart: tool rows, a group's members under its summary, a
//   one-paragraph commentary over the call it introduces, and any row that
//   hangs on an elbow under the row it answers (a decision record, an
//   interrupted or failed turn's end). A call and its own `⎿` result are
//   one unit, with no step between them. Paragraphs inside an answer sit a
//   block step apart too (`PROSE_GAP`, 0.86em), so an answer's paragraphs
//   and the blocks around it read as siblings of one turn.
// - **The rhythm scales with the reading size.** The turn and block steps
//   and the prose gaps are em-proportional to the answer's size
//   (`reading_step`): exactly the tokens at Standard, 37/14 at Comfortable,
//   41/15 at Large, so a paragraph gap never outgrows the block step. The
//   row step spaces UI rows, which do not scale, and stays 4.
// - **Chosen once.** The space above a row is chosen at reconcile from the
//   row before it, its own kind and the reading size, and is part of the
//   row's identity, so a changed gap is a changed row and nothing is
//   measured per frame.
// - **The prompt anchors its turn.** The operator's line is the turn's
//   heading: the answer's size (`answer_text_size`/`answer_line_height`,
//   14/22 at Standard) at `W_LABEL` in `TEXT_STRONG` under the accent `❯`,
//   with no band, pill or hover ground, over answers at the same size,
//   regular, in `TEXT`. An answer's own H1/H2 may be larger: they head sections of
//   one answer, while the prompt heads the turn by place — the turn step
//   above it, the accent in the gutter — not by size. Structural rows (tool
//   calls, summaries) are `FS_UI`/`LH_UI`; the stamp and the trail are
//   `FS_SM`/`LH_META`, their changing digits tabular.

/// 66px — the tallest remnant of a cut row the transcript hides under its
/// top edge while it follows the tail (three prose lines): a prompt, a tool
/// row or a short paragraph cut under the head rule goes whole, so the
/// body reads from a whole row; a long block read mid-way stays, since
/// hiding more would open a void (rule 2.3.4).
pub const TRANSCRIPT_TOP_SNAP_MAX: f32 = 3.0 * LH_PROSE;
/// 32px — above every prompt but the first: the turn boundary. No rule is
/// drawn between turns; this space, the prompt's weight and the stamp do the
/// job.
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

/// An added diff line's code: `RUNNING` lifted a step to read on its wash.
pub const DIFF_ADDED_INK: u32 = 0xb4cfc0;
/// A removed diff line's code: `BLOCKED` lifted the same step.
pub const DIFF_REMOVED_INK: u32 = 0xddb5b0;
/// A hunk row's wash, 8% of its state hue: a multi-line wash never passes
/// 8%, and the sign and the code's ink carry the meaning. (`BLOCKED_WASH`
/// is for one-line uses only.)
pub const DIFF_ADDED_WASH: u32 = 0x8cb59d14;
pub const DIFF_REMOVED_WASH: u32 = 0xd2908914;
/// A diff card at the tool name's x (C1, under the call's first letter):
/// `RAISED`, `R_BLOCK`, 4px above it, the fence's 12px inline. Its columns
/// are `[number][8][sign][4][code]`: the number column (`TEXT_FAINT`) is as
/// wide as the largest number's digits (`CODE_CELL` each), the sign is one
/// whole-pixel mono cell in its row's code ink, and code keeps its
/// indentation and soft-wraps rather than being cut. `HUNK_PAD_Y` 4 is the
/// one deliberate difference from a fence (`CODE_PAD_Y` 10): the rows'
/// washes run edge to edge, and 4px keeps the first and last rows' washes
/// off the card's corners without a slab of empty ground.
pub const HUNK_PAD_X: f32 = CODE_PAD_X;
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

/// 20px — an invisible hit area, not a drawn thing: a disclosure's gutter
/// target (the whole row toggles too) and a prompt action's button.
pub const TOOL_DISCLOSURE_HIT: f32 = GUTTER_W;
// (end WP-A) — append above this line only

// ======================================== WP-B · markdown, prose, scrollbars
// Owner: WP-B (rich.rs, scrollbar.rs, attachments::inline_file, the Markdown vendor knobs.)
// Edit values and append tokens only inside this section.

/// **Markdown.** Agent prose is Geist in `TEXT` at the reading size
/// (`answer_text_size`, set by the answer row), its paragraphs, list items
/// and quotes held to `PROSE_MEASURE`; code, tables and diffs keep the whole
/// column. Blocks sit `PROSE_GAP` apart; a heading takes more space above
/// (`PROSE_GAP + HEADING_SPACE_ABOVE` = 20) than below (`HEADING_SPACE_BELOW`
/// = 8). H1–H3 are `W_STRONG` `TEXT_STRONG`, H4–H6 `W_LABEL` `TEXT_STRONG`
/// (set apart from prose by weight, never dimmer than it), never italic or
/// underlined, each a whole pixel size on its own pixel line
/// (`prose_line_height`). A table is its header rule alone: no row rules, a
/// `W_BODY` `TEXT_MUTED` header, cells at the UI size, figures tabular. A
/// quote is a 2px `HAIRLINE_STRONG` rule and `TEXT_2`, not italic; a
/// thematic break is one `HAIRLINE`. List and quote text share one hang,
/// `PROSE_HANG`, with markers `TEXT_MUTED` right-aligned in it. Code is a
/// `RAISED` block whose language and `Copy` are a hover overlay, never a
/// header row; **inline code is mono `FS_UI` on a neutral `INLINE_CODE_WASH`
/// chip**, in `TEXT` at weight 400 whatever it sits in; links are `ACCENT`
/// over an `ACCENT_EDGE` underline.
///
/// 12px — between Markdown blocks (`SPACE_3`), the transcript's block
/// step. This and the heading spaces are Standard values; other reading
/// sizes scale them with `reading_step`.
pub const PROSE_GAP: f32 = SPACE_3;
/// 8px — added above a heading that follows a sibling, on top of
/// `PROSE_GAP`, so a heading opens a section rather than closing one.
pub const HEADING_SPACE_ABOVE: f32 = SPACE_2;
/// 8px — below a heading, in place of `PROSE_GAP`.
pub const HEADING_SPACE_BELOW: f32 = SPACE_2;
/// Inline code's ink on its chip: body ink, never brighter than the prose
/// around it (the Markdown path paints the chip; the plain-text fallback
/// carries the ink alone).
pub const INLINE_CODE_INK: u32 = TEXT;
/// `#ffffff0f` (6%) — inline code's chip: a neutral ground that shows the
/// copy boundary (`None`, `nav.rs`) without tinting the line.
pub const INLINE_CODE_WASH: u32 = 0xffffff0f;
/// The inline-code chip reaches 2px past its glyphs. Painted, never laid
/// out; its height is `inline_code_chip_h`, centred in the prose line box.
pub const INLINE_CODE_OVERHANG: f32 = SPACE_0_5;

/// Inline code's size at each reading size: the UI size at 14 (`FS_UI`, so
/// a code cell is `CODE_CELL`), 1.5 under the prose up to 15 and 2 under it
/// above. Tables set their cells at the same size.
pub fn inline_code_size(size: ferrite_core::settings::ReadingSize) -> f32 {
    let px = f32::from(size.px());
    if px <= 15. {
        px - (FS_PROSE - FS_UI)
    } else {
        px - 2.
    }
}

/// The inline-code chip's height: 4px over the prose size (18 at 14).
pub fn inline_code_chip_h(size: ferrite_core::settings::ReadingSize) -> f32 {
    f32::from(size.px()) + 4.
}

/// How far the chip stays inside the prose line box, top and bottom:
/// `(answer_line_height − inline_code_chip_h) / 2`.
pub fn inline_code_inset_y(size: ferrite_core::settings::ReadingSize) -> f32 {
    (answer_line_height(size) - inline_code_chip_h(size)) / 2.
}

/// A table row's line box: 6px over the prose size, so at 14 it is `LH_UI`
/// 20 and a row is 4 + 20 + 4 = 28, the list pitch.
pub fn table_line_height(size: ferrite_core::settings::ReadingSize) -> f32 {
    f32::from(size.px()) + 6.
}

/// 570px — the prose measure (~88 characters of Geist at 14px): the most a
/// paragraph, a list item or a quote runs before it wraps. Fixed, not scaled
/// by the reading size. Code, tables, diffs, tool rows and the Composer keep
/// the whole `READING_MAX_W` column.
pub const PROSE_MEASURE: f32 = 570.0;
/// 28px — the one hang lists and quotes share at Standard: bullet and
/// ordered text start this far in, their markers right-aligned inside it
/// `LIST_MARKER_GAP` from the text (only a list whose ordinals reach 100
/// widens), and a quote's text lands on the same x past its rule. Each
/// nesting level adds another. Scales with the reading size (`reading_step`:
/// 28 · 32 · 36).
pub const PROSE_HANG: f32 = 28.0;
/// 6px — between a list marker and its text.
pub const LIST_MARKER_GAP: f32 = SPACE_1_5;
/// A quote's rule. Its text inset is the hang less the rule
/// (`PROSE_HANG − QUOTE_RULE_W`, 26 at Standard), so quoted text starts
/// where list text does.
pub const QUOTE_RULE_W: f32 = 2.0;
/// 4px — a table cell's block padding, so a Standard row is 28px (its
/// inline padding is the vendor's 8px, which its column measurement
/// assumes).
pub const TABLE_CELL_PAD_Y: f32 = SPACE_1;
/// The rule under a table's header row: one step stronger than the rows'.
pub const TABLE_HEAD_RULE: u32 = HAIRLINE_STRONG;
/// 4px — a horizontal rule's own margin inside its block, so it sits 16px
/// from its neighbours.
pub const RULE_MARGIN_Y: f32 = SPACE_1;
/// A fenced code block: 12px inline, 10px block padding (Zeron's code body;
/// 10 is off the scale so a one-line block is 10 + 18 + 10 = 38, and the
/// hover overlay's 24px actions centre on its first line).
pub const CODE_PAD_X: f32 = SPACE_3;
pub const CODE_PAD_Y: f32 = 10.0;
/// A fence's actions overlay: the language id, html `Preview`,
/// `Copy`/`Copied`, top-right over the block. It is always laid out (so it
/// never moves the block) and only shown under the pointer (the 150ms
/// hover blend), while its keys have focus, or while the caret or a
/// selection is inside the block — those two instantly.
pub const CODE_ACTIONS_TOP: f32 = 7.0;
pub const CODE_ACTIONS_RIGHT: f32 = SPACE_1;
/// Code actions keep a stable target when Copy becomes Copied.
pub const CODE_ACTION_H: f32 = 24.;
pub const CODE_ACTION_MIN_W: f32 = 56.;
pub const CODE_ACTION_PAD_X: f32 = SPACE_2;
/// The html preview dialog: the reading column's width, and a height cap
/// before its body scrolls.
pub const HTML_PREVIEW_MAX_H: f32 = 520.0;
/// An inline file chip: `CHIP_H` tall so it fits a 22px prose line without
/// moving it; 6px inline padding; no file mark (an image leads with its
/// 14px thumbnail, 6px from the name); clamped between 64 and 280px wide.
pub const INLINE_FILE_H: f32 = CHIP_H;
pub const INLINE_FILE_PAD_X: f32 = SPACE_1_5;
pub const INLINE_FILE_GAP: f32 = SPACE_1_5;
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
/// 24px — the empty board's Ferrite mark (`ferrite-mono` in `TEXT_FAINT`,
/// rule 2.11.4), left-aligned on the keycap column, `EMPTY_BOARD_MARK_GAP`
/// (16px) above the hints. It replaces the line of words: the board says
/// how to start, once.
pub const EMPTY_BOARD_MARK: f32 = 24.0;
pub const EMPTY_BOARD_MARK_GAP: f32 = SPACE_4;
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
/// The usage ring: a 14px box, 5.4px radius, 2px stroke, sweeping
/// clockwise from 12 o'clock with a round cap. No text, ever.
pub const USAGE_RING_D: f32 = 14.0;
pub const USAGE_RING_R: f32 = 5.4;
pub const USAGE_RING_W: f32 = 2.0;
/// Between the meter's three rings: tight enough that the trio reads as
/// one control, wide enough that the three readings stay separate.
pub const USAGE_RING_GAP: f32 = 4.0;
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
/// until the provider says so. Below tight a status-line ring is `TEXT_2`:
/// colour is state, and a context half full is not a state.
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
/// Subagent's alike, rule 2.8): `RAISED`, one 1px `COMPOSER_EDGE`,
/// `R_BLOCK`, in the reading column. Docked on a live Composer it merges
/// into it — one outlined block: the Decision section, one full-width
/// seam (the Composer's own top edge, held in layout so nothing moves),
/// the Composer's mono input line — so there is no gap and no second
/// outline. Ochre is only on `◆` and the kind word
/// (`◆ approval · Bash`, `FS_SM`, the word `W_BODY` `ATTENTION`, each
/// detail after a `TEXT_FAINT` `·` in `TEXT_MUTED`): no wash, no state
/// edge. Prose question (Geist `FS_PROSE` `W_STRONG` `TEXT_STRONG`), option
/// rows that each show the one key that picks them. Deny is not red. 12px
/// inline and 10px block padding; the block step (12) between sections.
/// The head names the kind and nothing else unless a status adds something
/// (`sending`, `work continues`); a card is waiting by being there. Every
/// section keeps its natural height — nothing clips; when the Pane runs
/// short the prose goes first, then the command well gives way down to one
/// line.
pub const DECISION_PAD_X: f32 = SPACE_3;
pub const DECISION_PAD_Y: f32 = 10.0;
pub const DECISION_GAP: f32 = SPACE_3;
/// The head's drawn diamond: 8px in the glyph box of a `LH_META` line.
pub const DECISION_MARK: f32 = SPACE_2;
/// An option row (rule 2.8.4): a title-only row is a list row,
/// `MENU_ROW_H` — `LH_UI` plus 4px above and below — and a description
/// adds `LH_PROSE_SM` a line; rows sit flush. It hangs 8px left of the
/// card's content so its hover ground reaches around the keycap, which is
/// centred on the glyph column; the title starts 8px after the glyph box,
/// on the text column. Hover is `RAISED_2`, selected is `FILL` plus a
/// trailing `ACCENT` check — never a focus-coloured border.
pub const DECISION_ROW_PAD_X: f32 = SPACE_2;
pub const DECISION_ROW_PAD_Y: f32 = (MENU_ROW_H - LH_UI) / 2.0;
pub const DECISION_ROW_GAP: f32 = 0.0;
pub const DECISION_ROW_INNER_GAP: f32 = GUTTER_GAP;
/// The selected row's trailing check.
pub const DECISION_CHECK: f32 = SPACE_3;
/// A question's text to its rows, and one question to the next.
pub const DECISION_QUESTION_GAP: f32 = SPACE_2;
pub const DECISION_QUESTIONS_GAP: f32 = SPACE_4;
/// The command well (C5): `RAISED_2` — code one step up from its card,
/// never darker than the Pane — `R_CONTROL`, 6/10 padding, mono
/// `TEXT_STRONG`, a shell command after a `TEXT_FAINT` `$ ` that copy
/// leaves out. It scrolls past 160px, and when the Pane is short it is
/// the one section that shrinks, never below one line
/// (`DECISION_WELL_MIN_H`).
pub const DECISION_WELL_PAD_X: f32 = 10.0;
pub const DECISION_WELL_PAD_Y: f32 = SPACE_1_5;
pub const DECISION_WELL_MIN_H: f32 = LH_CODE + 2.0 * DECISION_WELL_PAD_Y;
pub const DECISION_INPUT_MAX_H: f32 = 160.0;
/// A question body's scroll cap inside the card (head and footer stay
/// pinned); container-relative, never a window fraction.
pub const DECISION_BODY_MAX_H: f32 = 320.0;
/// Below a 360px Pane the body caps at two described option rows and
/// scrolls, so the head and the answer row always stay in reach.
pub const DECISION_SHORT_PANE_H: f32 = 360.0;
pub const DECISION_SHORT_BODY_MAX_H: f32 = 2.0 * (MENU_ROW_H + LH_PROSE_SM) + DECISION_ROW_GAP;
/// The scroll gutter a body keeps free for its thumb.
pub const DECISION_SCROLL_GUTTER: f32 = SPACE_1;
/// A question's own answer (rule 2.8.6) is not a second field: it is one
/// bare mono input line on the text column, `LH_UI` high, placeholder
/// `Or type your own answer…` in `TEXT_MUTED`; the digit one past the last
/// option arms it.
pub const QUESTION_OTHER_H: f32 = LH_UI;
/// The primary's `↵`: `ON_ACCENT` at 70%, so the key reads under its label.
pub const SEND_KEY_INK: u32 = 0xffffffb3;
/// An L2 keycap pair (`y allow`): key, 4px, verb; pairs 12px apart.
pub const DECISION_KEY_GAP: f32 = SPACE_1;
pub const DECISION_KEYS_GAP: f32 = SPACE_3;
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
//   first paint and scroll-back never animate. One breath,
//   `MOTION_BREATH_MS` 2400, used only by unread. Nothing on a
//   high-frequency interaction scales, slides or staggers.
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
        assert_eq!(NAV_ROW_H, 2.0 * SPACE_1 + LH_UI);
        assert_eq!(THREAD_ROW_H, NAV_ROW_H);
        assert_eq!(GROUP_ROW_H, NAV_ROW_H);
        assert_eq!(THREAD_ROW_H, MENU_ROW_H, "a nav row is a menu row's box");
        assert_eq!(THREAD_ROW_H, 28.0, "one list pitch across the app (C9)");
    }

    #[test]
    fn every_type_role_has_a_whole_pixel_line_box() {
        use ferrite_core::settings::ReadingSize;
        for (size, line) in [
            (FS_PROSE, LH_PROSE),
            (FS_PROSE_SM, LH_PROSE_SM),
            (FS_UI, LH_UI),
            (FS_UI, LH_CODE),
            (FS_SM, LH_META),
        ] {
            assert_eq!(line, line.round());
            assert!(line >= size * 1.25, "{size}px on a {line}px line box");
        }
        for reading in ReadingSize::STEPS.map(ReadingSize::nearest) {
            let (size, line) = (answer_text_size(reading), answer_line_height(reading));
            assert_eq!(line, line.round());
            assert!(line >= size * 1.5, "{reading:?}: {size}/{line}");
        }
        assert_eq!(answer_text_size(ReadingSize::STANDARD), FS_PROSE);
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
