//! The Soft visual system — every token the prototype resolves, named once.
//!
//! Values are transcribed from the approved HTML prototype
//! (`nav-soft-surfaces.prototype.html`, `mode=soft type=sans`) via the
//! measured 569-node computed-style dump, not from a comp. Render code
//! (`pane.rs`, `cockpit.rs`, `composer.rs`, `nav.rs`, `icons.rs`) imports
//! from here and holds no color or metric literal of its own; core stays
//! color-blind.
//!
//! Solid colors are `0xRRGGBB` and drawn with `gpui::rgb`; translucent ones
//! are `0xRRGGBBAA` and drawn with `gpui::rgba`. The alpha byte is the
//! prototype's fraction × 255, rounded.
//!
//! Soft draws **no hairline separators at all**. There is no border between
//! the nav and the Cockpit, no rule above the changed strip, none above the
//! Composer, none under the Pane head. Separation is by fill contrast only.

// ---------------------------------------------------------------- grounds

/// `#0e0e0e` — `--ground`: the Cockpit field and the gutters between Panes.
/// The darkest surface, and the window's own background.
pub const GROUND: u32 = 0x0e0e0e;
/// `#171717` — `--pane`: a Pane's own ground.
pub const PANE: u32 = 0x171717;
/// `#232323` — `--nav`: the whole 286px navigation column, window-chrome
/// band included. The *lightest* field in the system — navigation reads as
/// nearer than the Cockpit, which is the inversion the Soft mode makes.
#[allow(dead_code)]
pub const NAV: u32 = 0x232323;
/// `#282828` — `--raised`: inline-code chips, code blocks, keycaps, the
/// changed strip's file chips, and the Composer's own ground.
pub const RAISED: u32 = 0x282828;
/// `#282828` — `--menu`: the floating menu ground. Same value as `RAISED`,
/// kept as its own name because the prototype declares two roles and a
/// future retune can split them without a rename.
#[allow(dead_code)]
pub const MENU: u32 = 0x282828;
/// `#2c2c2c` — `--hover`: every control's hover face, the nav row hover,
/// and the mode chip's *resting* ground.
pub const HOVER: u32 = 0x2c2c2c;
/// `#343434` — `--fill`: the selected fill. It lands on the Group row and
/// on a collapsed rail item, never on a Thread row.
pub const FILL: u32 = 0x343434;
/// `#3b3b3b` — `--fill-hover`: a selected Group row under the pointer.
pub const FILL_HOVER: u32 = 0x3b3b3b;
/// `rgba(255,255,255,0.13)` — `--meter-off`: an unlit tasks-meter segment,
/// and the context ring's track.
#[allow(dead_code)]
pub const METER_OFF: u32 = 0xffffff21;

// -------------------------------------------------------- rails and lines

/// `#545454` — `--group-rail`: the 1px vertical rail that indents a Group's
/// member Threads. **The only line the Soft mode draws.** Pixel-verified in
/// the target render at x = 21: `(84, 84, 84)`, with `(35,35,35)` either
/// side.
#[allow(dead_code)]
pub const GROUP_RAIL: u32 = 0x545454;

/// Fully transparent — a Pane's resting `--pane-edge`. The Pane's 1px
/// border is **always** in layout; only its color changes, so nothing
/// reflows when a Decision or a blocker arrives.
pub const TRANSPARENT: u32 = 0x00000000;

// -------------------------------------------------------------------- ink

/// `#ffffff` — `--text-strong`: the active Group title, a Pane head's
/// Thread id, body headings and bold runs, a Decision's subject.
pub const TEXT_STRONG: u32 = 0xffffff;
/// `#dedede` — `--text`: the filter label, Thread row titles, links, a tool
/// event's verb, a keycap's bold key.
#[allow(dead_code)]
pub const TEXT: u32 = 0xdedede;
/// `#a8a8a8` — `--text-2`: body prose, the Project line, a lit meter
/// segment, the context ring's used arc, the Composer's own text and caret.
pub const TEXT_2: u32 = 0xa8a8a8;
/// `#959595` — `--text-muted`: the checkout line, the Pane head, the tasks
/// strip, tool arguments and durations, hints, the parked status dot.
pub const TEXT_MUTED: u32 = 0x959595;
/// `#9e9e9e` — `--text-on-fill`: a checkout line sitting on the selected
/// fill. **Paints nowhere in this prototype** — the fill only ever lands on
/// a Group row, and Group rows carry no checkout line. Kept named so the
/// rule survives if Thread rows ever become selectable.
#[allow(dead_code)]
pub const TEXT_ON_FILL: u32 = 0x9e9e9e;
/// `#6e6e6e` — `--sep`: the `·` seam, an event's `▸`/`●` glyph, a result's
/// `└` elbow, hunk line numbers, and a link's underline.
pub const SEP: u32 = 0x6e6e6e;

// -------------------------------------------------------- state + signals

/// `#9a9a9a` — `--focus`: the focused Pane's 2px ring, and every
/// `:focus-visible` outline. A quiet neutral, never an accent hue: the
/// system has no accent.
pub const FOCUS: u32 = 0x9a9a9a;
/// `#7fbf95` — `--running`: the running status dot, a running signal line,
/// the pass chip, diff `+`.
pub const RUNNING: u32 = 0x7fbf95;
/// `rgba(127,191,149,0.11)` — the pass chip's ground and an added hunk row.
pub const RUNNING_WASH: u32 = 0x7fbf951c;
/// `#d9b872` — `--attention`: a Decision. The status dot, the signal line,
/// the Pane's border, the Decision card's mark.
pub const ATTENTION: u32 = 0xd9b872;
/// `rgba(217,184,114,0.10)` — the Decision card's ground.
pub const ATTENTION_WASH: u32 = 0xd9b8721a;
/// `rgba(217,184,114,0.26)` — the Decision card's 1px inset ring. An inset
/// ring, not a border: it takes no layout.
pub const ATTENTION_EDGE: u32 = 0xd9b87242;
/// `#e08f86` — `--blocked`: the blocked status dot and signal line, the
/// Pane's border, diff `−`.
pub const BLOCKED: u32 = 0xe08f86;
/// `rgba(224,143,134,0.11)` — a removed hunk row's ground.
pub const BLOCKED_WASH: u32 = 0xe08f861c;
/// The idle/parked status dot — the muted ink in a dot role. An alias, not
/// a fresh value: the prototype reuses `#959595` for both, and the signal
/// keeps its own name so a future retune can split them without a rename.
pub const IDLE: u32 = TEXT_MUTED;

/// `#10a37f` — `--provider-codex`: the Codex logomark's fill.
#[allow(dead_code)]
pub const PROVIDER_CODEX: u32 = 0x10a37f;
/// `#d97757` — `--provider-claude`: the Claude logomark's fill.
#[allow(dead_code)]
pub const PROVIDER_CLAUDE: u32 = 0xd97757;

// ------------------------------------------------- transcript colour (app)

// The prototype keeps its transcript nearly monochrome: one syntax class,
// grey diff bodies, inherited-ink inline code, a `--sep` underline. The
// operator overruled that (2026-09) — a cockpit reads faster with the
// transcript's structure coloured — so the tokens below are Ferrite's own,
// not transcriptions. Each stays close to the palette it joins.

/// `#9fd4b1` — an added diff line's code. `--running` lifted a step so a
/// whole line of it reads on the green wash; the sign column keeps
/// `--running` itself.
pub const DIFF_ADDED_INK: u32 = 0x9fd4b1;
/// `#eda59d` — a removed diff line's code: `--blocked` lifted the same step.
pub const DIFF_REMOVED_INK: u32 = 0xeda59d;
/// `#a9b4f5` — a keyword in a fenced block. The one hue the palette lacks,
/// a soft blue-violet, so keywords never read as a state.
pub const SYN_KEYWORD: u32 = 0xa9b4f5;
/// `#8fcda3` — a string literal: the palette's green a shade lighter than
/// `--running`, so a quoted run does not read as a pass verdict.
pub const SYN_STRING: u32 = 0x8fcda3;
/// `#d9b872` — a number literal: `--attention` in a type role.
pub const SYN_NUMBER: u32 = ATTENTION;
/// `#e3c88f` — inline code's ink on its `--raised` chip: a light amber, so
/// a path or a flag stands out of the prose it sits in.
pub const INLINE_CODE_INK: u32 = 0xe3c88f;
/// `#8ab4f8` — a link's ink, and its underline. Inert still — nothing opens.
pub const LINK_INK: u32 = 0x8ab4f8;
/// `rgba(217,119,87,0.10)` — the operator's prompt block on a Claude Thread.
pub const PROMPT_WASH_CLAUDE: u32 = 0xd977571a;
/// `rgba(16,163,127,0.10)` — the operator's prompt block on a Codex Thread.
pub const PROMPT_WASH_CODEX: u32 = 0x10a37f1a;
/// 2px — the prompt block's left edge in the provider's colour.
pub const PROMPT_EDGE_W: f32 = 2.0;

/// `#7fbf95` at 1px inset — a nav row that will accept the drag.
#[allow(dead_code)]
pub const DROP_VALID: u32 = RUNNING;
/// `#e08f86` at 1px inset — a nav row that refuses it.
#[allow(dead_code)]
pub const DROP_REFUSED: u32 = BLOCKED;
/// `#ffffff14` — the seam's grab band under the pointer: a faint lift
/// over the gutter that says "drag here".
pub const SEAM_HOVER: u32 = 0xffffff14;
/// `#7fbf9526` — the wash over the slot a dragged Pane would take.
pub const DROP_WASH: u32 = 0x7fbf9526;
/// `0.4` — a row's opacity while it is being dragged.
#[allow(dead_code)]
pub const DRAGGING_OPACITY: f32 = 0.4;

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

// ------------------------------------------------------- app-only mappings

/// `#3f3f3f` — `::selection` background; `#ffffff` foreground.
pub const SELECTION: u32 = 0x3f3f3f;
/// The pressed shade — one step past `FILL`, the only value the prototype
/// offers above it. An alias like `IDLE`: the prototype declares no press
/// state (its only `:active` is a 0.96 scale on the collapse button), so a
/// future retune can split them without a rename.
pub const PRESSED: u32 = FILL_HOVER;
/// `#3a3a3a` — `--scrollbar`: the nav-tree and Pane-body thumb.
#[allow(dead_code)]
pub const SCROLLBAR: u32 = 0x3a3a3a;

// ------------------------------------------------------------------- type

/// 13px — `--fs-lg`: the filter label, a Group row's title, a Pane head's
/// Thread id.
#[allow(dead_code)]
pub const FS_LG: f32 = 13.0;
/// 12px — `--fs-md`: a Thread row's title, filter options, everything read
/// or typed in a Pane (prose, prompts, tool rows, results, code, diffs,
/// the Composer), body headings, a Decision's subject.
pub const FS_MD: f32 = 12.0;
/// 11px — `--fs-sm`: the Project and checkout lines, the tasks strip, tool
/// events, the pass chip, the Composer and its controls.
pub const FS_SM: f32 = 11.0;
/// 11px — `--fs-mono`, retuned: the prototype set 10.5px here and used it
/// for code, arguments and results too. The operator ruled the half-pixel
/// step out — on a 12px mono face it read as a third, smaller text — so
/// everything read in a Pane (prose, prompts, tool rows, results, code,
/// diffs, the Composer) now sits on `FS_MD`, and this size is for meta
/// only: durations, chips, hints, keycaps, the head's checkout, the
/// changed strip, and the L2/L3 cells' lines.
pub const FS_MONO: f32 = 11.0;

/// 1.25 — nav row titles and the Project/checkout lines.
#[allow(dead_code)]
pub const LINE_TIGHT: f32 = 1.25;
/// 1.45 — chrome and controls: the filter label, the Pane head, the tasks
/// strip, keycaps, the changed strip, the Composer.
#[allow(dead_code)]
pub const LINE_UI: f32 = 1.45;
/// 1.55 — reading: Pane body prose, tool events, result lines, code blocks.
pub const LINE_BODY: f32 = 1.55;
/// 1.65 — diff hunk rows.
#[allow(dead_code)]
pub const LINE_HUNK: f32 = 1.65;

/// 0.84px — the *only* letter-spacing in Soft + Sans: `CHANGED` at 0.08em
/// on 10.5px. gpui has no tracking; see the port note on `changed_strip`.
#[allow(dead_code)]
pub const CHANGED_TRACKING: f32 = 0.84;
/// 0.6em — JetBrains Mono's advance width, the pitch a tracked mono label
/// must be laid out on so a per-character cell cannot round up to a whole
/// pixel.
#[allow(dead_code)]
pub const MONO_ADVANCE: f32 = 0.6;

// --------------------------------------------------------- geometry: shell

/// 286px — the navigation column, and 56px for the rail ⌘B folds it to.
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input.
#[allow(dead_code)]
pub const NAV_WIDTH: f32 = 286.0;
#[allow(dead_code)]
pub const NAV_RAIL_WIDTH: f32 = 56.0;
/// 42px — the window-chrome band at the top of the nav (traffic lights and
/// the collapse button). **The Cockpit has no band of any kind above it:**
/// the Pane grid starts at y = 0.
#[allow(dead_code)]
pub const WIN_CHROME_H: f32 = 42.0;
/// 42px — the nav head band, which holds the Project filter.
#[allow(dead_code)]
pub const NAV_HEAD_H: f32 = 42.0;
/// The nav tree's padding: 8px top and inline, 16px bottom.
#[allow(dead_code)]
pub const NAV_TREE_PAD: f32 = 8.0;
#[allow(dead_code)]
pub const NAV_TREE_PAD_B: f32 = 16.0;
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

/// The Pane board: 8px gap on both axes, 10px padding on all four sides.
/// (The prototype's own render reserves 58px at the bottom for its
/// mode-switcher; that is prototype-only chrome and its `data-view="window"`
/// rule restores 10px. Port 10px.)
pub const GRID_GAP: f32 = 8.0;
pub const GRID_PAD: f32 = 10.0;

// ---------------------------------------------------------- geometry: nav

/// 28px — the collapse button, the rail's filter button, a rail item, and
/// the Project filter trigger all share this height.
#[allow(dead_code)]
pub const ICON_BUTTON: f32 = 28.0;
#[allow(dead_code)]
pub const FILTER_TRIGGER_H: f32 = 28.0;
/// The filter menu: 4px of padding around 30px rows, offset 38px below the
/// nav head's top edge, inset 8px each side.
pub const MENU_PAD: f32 = 4.0;
pub const MENU_ROW_H: f32 = 30.0;
#[allow(dead_code)]
pub const MENU_TOP: f32 = 38.0;
/// A nav row's padding — 6px block, 8px inline — and the 1px gap between
/// its stacked lines.
#[allow(dead_code)]
pub const ROW_PAD_X: f32 = 8.0;
#[allow(dead_code)]
pub const ROW_PAD_Y: f32 = 6.0;
#[allow(dead_code)]
pub const ROW_GAP: f32 = 1.0;
/// 254px — the content box of a root-level nav row: the column less the
/// tree's inline padding, less the row's own. A truncating title has to be
/// pinned to it, because gpui only measures an ellipsis against a width it
/// knows on the line's very first measure (see `nav::group_row`).
#[allow(dead_code)]
pub const ROW_TEXT_W: f32 = NAV_WIDTH - 2.0 * NAV_TREE_PAD - 2.0 * ROW_PAD_X;
/// 43px — a Group parent row: 6 + 16.25 + 1 + 13.75 + 6.
#[allow(dead_code)]
pub const GROUP_ROW_H: f32 = 43.0;
/// 56.5px — a Thread row: 6 + 15 + 1 + 13.75 + 1 + 13.75 + 6.
#[allow(dead_code)]
pub const THREAD_ROW_H: f32 = 56.5;
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
/// The provider logomark in a nav row (14px) and in the model picker (12px).
#[allow(dead_code)]
pub const PROVIDER_MARK: f32 = 14.0;
#[allow(dead_code)]
pub const PROVIDER_MARK_SM: f32 = 12.0;
/// The folder and branch marks on the Project and checkout lines, and the
/// 5px gap to their labels.
#[allow(dead_code)]
pub const ROW_ICON: f32 = 12.0;
#[allow(dead_code)]
pub const ROW_ICON_GAP: f32 = 5.0;

// --------------------------------------------------------- geometry: pane

/// 32px — the Pane head. No background, no border, no rule beneath it.
pub const PANE_HEAD_H: f32 = 32.0;
/// 24px — the tasks strip, and the changed-files strip.
pub const TASKS_STRIP_H: f32 = 24.0;
#[allow(dead_code)]
pub const CHANGED_STRIP_H: f32 = 24.0;
/// 12px — the inline padding every Pane strip shares.
pub const PANE_PAD_X: f32 = 12.0;
/// The Pane body's padding: 6px top, 12px inline, 12px bottom.
pub const BODY_PAD_T: f32 = 6.0;
#[allow(dead_code)]
pub const BODY_PAD_B: f32 = 12.0;
/// 6px — the Pane head's status dot.
pub const STATUS_DOT: f32 = 6.0;
/// The tasks meter: 12 × 4 segments, 1px radius, 3px apart (15px pitch).
#[allow(dead_code)]
pub const METER_SEG_W: f32 = 12.0;
#[allow(dead_code)]
pub const METER_SEG_H: f32 = 4.0;
#[allow(dead_code)]
pub const METER_SEG_GAP: f32 = 3.0;
#[allow(dead_code)]
pub const METER_SEG_R: f32 = 1.0;
/// Radii: 8 the Pane surface · 6 controls and rows · 4 chips and cards ·
/// 3 inline code. The filter menu is `6 + 4 = 10`.
#[allow(dead_code)]
pub const R_SURFACE: f32 = 8.0;
#[allow(dead_code)]
pub const R_CONTROL: f32 = 6.0;
pub const R_CHIP: f32 = 4.0;
pub const R_TIGHT: f32 = 3.0;
#[allow(dead_code)]
pub const R_MENU: f32 = 10.0;
/// The context ring: a 14px box, 5.4px radius, 2px stroke, sweeping
/// clockwise from 12 o'clock with a round cap. No text, ever.
pub const USAGE_RING_D: f32 = 14.0;
#[allow(dead_code)]
pub const USAGE_RING_R: f32 = 5.4;
pub const USAGE_RING_W: f32 = 2.0;
/// The focused Pane's ring: 2px wide, 2px outside the Pane's border box.
/// gpui has no `outline-offset`, so it is an absolutely positioned overlay
/// at `inset(-4px)` inside a non-clipping wrapper; its radii follow the
/// offset — inner 10, outer 12.
pub const FOCUS_RING_W: f32 = 2.0;
pub const FOCUS_RING_OFFSET: f32 = 2.0;
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
/// Keycaps: 3px block, 7px inline padding, 5px apart.
#[allow(dead_code)]
pub const KBD_PAD_X: f32 = 7.0;
#[allow(dead_code)]
pub const KBD_PAD_Y: f32 = 3.0;
#[allow(dead_code)]
pub const KEYS_GAP: f32 = 5.0;
/// The Composer: 7px top, 12px inline, 8px bottom padding; two 20px rows
/// 3px apart; a 2 × 14 caret.
#[allow(dead_code)]
pub const COMPOSER_PAD_T: f32 = 7.0;
#[allow(dead_code)]
pub const COMPOSER_PAD_B: f32 = 8.0;
pub const COMPOSER_ROW_H: f32 = 20.0;
#[allow(dead_code)]
pub const COMPOSER_GAP: f32 = 3.0;
pub const CARET_W: f32 = 2.0;
#[allow(dead_code)]
pub const CARET_H: f32 = 14.0;
/// The Composer's controls: a 20px mode chip with 7px inline padding and a
/// 10px pencil; a 20px model picker with 6px/4px padding and a 12px chevron.
pub const CHIP_H: f32 = 20.0;
#[allow(dead_code)]
pub const MODE_CHIP_PAD_X: f32 = 7.0;
#[allow(dead_code)]
pub const ICON_PENCIL: f32 = 10.0;
#[allow(dead_code)]
pub const PICKER_PAD_L: f32 = 6.0;
#[allow(dead_code)]
pub const PICKER_PAD_R: f32 = 4.0;
#[allow(dead_code)]
pub const ICON_CHEVRON: f32 = 12.0;
#[allow(dead_code)]
pub const ICON_CHEVRON_LG: f32 = 14.0;
/// 16px — the collapse and rail-filter buttons' icon, centred in 28px.
#[allow(dead_code)]
pub const ICON_BUTTON_GLYPH: f32 = 16.0;

// ---------------------------------------------- geometry: transcript body

/// 9px — an event's glyph column, and 8px to the verb beside it. Their sum,
/// 17px, is the inset a result line and a hunk share so both land under the
/// verb's first character. Keep the relationship, not just the numbers.
pub const GUTTER_W: f32 = 9.0;
#[allow(dead_code)]
pub const EVENT_GAP: f32 = 8.0;
pub const INDENT: f32 = 17.0;
/// A tool row's vertical padding. The prototype's 3px each side put 43px
/// between consecutive calls; a run of shell commands reads as a list only
/// when they sit as close as Claude Code's own `●`/`⎿` pairs do.
pub const EVENT_PAD_Y: f32 = 1.0;
/// The result line's padding: hugging its call above, a hair under.
pub const RESULT_PAD_T: f32 = 0.0;
pub const RESULT_PAD_B: f32 = 1.0;
/// An invisible hit area, not a drawn thing: the tool-disclosure target.
pub const TOOL_DISCLOSURE_HIT: f32 = 20.0;
/// Paragraph and list rhythm: 10px below a paragraph, a list or a code
/// block; 12/6 around a heading; 3px between list items; a 16px list indent
/// with a 4px disc 15px left of the text.
#[allow(dead_code)]
pub const P_MARGIN_B: f32 = 10.0;
#[allow(dead_code)]
pub const H4_MARGIN_T: f32 = 12.0;
#[allow(dead_code)]
pub const H4_MARGIN_B: f32 = 6.0;
#[allow(dead_code)]
pub const LI_GAP: f32 = 3.0;
#[allow(dead_code)]
pub const UL_INDENT: f32 = 16.0;
#[allow(dead_code)]
pub const BULLET_D: f32 = 4.0;
#[allow(dead_code)]
pub const BULLET_OFFSET: f32 = 15.0;
/// Code blocks: a language label at 5/10/0, then `pre` at 4/10/8.
#[allow(dead_code)]
pub const CODE_LANG_PAD_T: f32 = 5.0;
#[allow(dead_code)]
pub const CODE_PAD_X: f32 = 10.0;
#[allow(dead_code)]
pub const CODE_PRE_PAD_T: f32 = 4.0;
#[allow(dead_code)]
pub const CODE_PRE_PAD_B: f32 = 8.0;
/// Inline code's own padding: 1px block, 4px inline.
#[allow(dead_code)]
pub const INLINE_CODE_PAD_X: f32 = 4.0;
#[allow(dead_code)]
pub const INLINE_CODE_PAD_Y: f32 = 1.0;
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
/// A chip's padding — the pass chip, the changed strip's file chip: 1px
/// block, 6px inline.
#[allow(dead_code)]
pub const CHIP_PAD_X: f32 = 6.0;
#[allow(dead_code)]
pub const CHIP_PAD_Y: f32 = 1.0;
#[allow(dead_code)]
pub const FILE_CHIP_GAP: f32 = 6.0;
/// The hand-drawn scrollbar: a 3px thumb inside a 9px gutter.
#[allow(dead_code)]
pub const SCROLLBAR_W: f32 = 9.0;
#[allow(dead_code)]
pub const SCROLLBAR_THUMB_W: f32 = 3.0;

// ------------------------------------- geometry: levels below L1 (unspecified)

/// The prototype specifies **only** the L1 Pane. Semantic zoom's Instruments
/// and Wall renderings keep the metrics they have; they inherit the new
/// palette and the new type scale through §1.1's map and are not otherwise
/// redesigned. Do not invent a Soft L2/L3 — flag it and leave it.
pub const CELL_HEADER_H: f32 = 24.0;
pub const CELL_PAD: f32 = 10.0;
pub const LED_WALL: f32 = 5.0;
pub const DONE_CELL_OPACITY: f32 = 0.75;
pub const DONE_WALL_OPACITY: f32 = 0.6;
/// 120ms ease-out — every hover/press transition the prototype declares.
/// gpui 0.2.2 refines styles without interpolation; recorded, not applied.
#[allow(dead_code)]
pub const TRANSITION_MS: u64 = 120;

// ------------------------------------------------------------------ faces

/// The mono face — **bundled**, not borrowed. Everything inside a Pane is
/// JetBrains Mono; everything outside it is the system UI sans. There is no
/// third family, and this is the only name that reaches it.
///
/// gpui has no variation-axis support, so the prototype's single variable
/// `400 700` face cannot be used: `main.rs` registers four static instances
/// instead — Regular, Medium, SemiBold, Bold.
///
/// **All four are one family, and the weight axis is how you pick one.**
/// The Medium and SemiBold files do declare their own name ID 1
/// (`JetBrains Mono Medium` / `… SemiBold`), but CoreText and font-kit key a
/// family on the *typographic* family, name ID 16, which is `JetBrains Mono`
/// for all four. Measured in the running app: `all_font_names()` reports the
/// single family `JetBrains Mono`, and shaping the same text at each weight
/// resolves four distinct faces —
///
/// | call | resolved face |
/// |---|---|
/// | `.font_family(FONT_MONO)` | `FontId(29)` — Regular |
/// | `+ .font_weight(FontWeight::MEDIUM)` | `FontId(30)` |
/// | `+ .font_weight(FontWeight::SEMIBOLD)` | `FontId(31)` |
/// | `+ .font_weight(FontWeight::BOLD)` | `FontId(32)` |
///
/// So weight 500 is `.font_weight(FontWeight::MEDIUM)` — `.event.task b`,
/// `.changed b`, a selected filter option — and weight 600 is
/// `.font_weight(FontWeight::SEMIBOLD)` — a Pane head's Thread id, body
/// headings and bold runs, a signal line, a tool event's verb, a keycap's
/// key, a Decision's subject.
///
/// Do **not** reach for a weight by family name. `.font_family("JetBrains
/// Mono Medium")` resolves to `FontId(41)` — the same id a deliberately
/// bogus family name returns, i.e. the fallback face. It fails silently, in
/// the fallback font, which is the exact trap the four static files were
/// bundled to avoid.
pub const FONT_MONO: &str = "JetBrains Mono";

/// The UI face for everything outside a Pane: the platform's system UI
/// font, which gpui resolves from this special name on both platforms.
/// This is the prototype's `-apple-system, BlinkMacSystemFont, …` stack.
/// Unlike the mono family it exposes a real weight axis, so `.font_weight`
/// works here.
pub const FONT_UI: &str = ".SystemUIFont";

/// Install Longbridge once per app, then map its semantic theme to Ferrite's
/// existing tokens. Constructors also call this for standalone test windows.
/// The window mounts the toolkit Root for Settings search input and focus.
pub fn init_components(cx: &mut gpui::App) {
    use gpui::{px, rgb, rgba};
    use gpui_component::{Theme, ThemeMode};

    if cx.has_global::<Theme>() {
        return;
    }
    gpui_component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.font_family = FONT_UI.into();
    theme.font_size = px(FS_MD);
    theme.mono_font_family = FONT_MONO.into();
    theme.mono_font_size = px(FS_MD);
    theme.radius = px(R_CONTROL);
    theme.radius_lg = px(R_MENU);
    theme.shadow = false;
    theme.background = rgb(PANE).into();
    theme.foreground = rgb(TEXT).into();
    theme.border = rgba(TRANSPARENT).into();
    theme.accent = rgb(HOVER).into();
    theme.accent_foreground = rgb(TEXT_STRONG).into();
    theme.secondary = rgb(RAISED).into();
    theme.secondary_hover = rgb(HOVER).into();
    theme.secondary_active = rgb(PRESSED).into();
    theme.secondary_foreground = rgb(TEXT_2).into();
    theme.primary = rgb(FILL).into();
    theme.primary_hover = rgb(FILL_HOVER).into();
    theme.primary_active = rgb(PRESSED).into();
    theme.primary_foreground = rgb(TEXT_STRONG).into();
    theme.muted = rgb(RAISED).into();
    theme.muted_foreground = rgb(TEXT_MUTED).into();
    theme.popover = rgb(MENU).into();
    theme.popover_foreground = rgb(TEXT).into();
    theme.ring = rgb(FOCUS).into();
    theme.selection = rgb(SELECTION).into();
    theme.sidebar = rgb(MENU).into();
    theme.sidebar_foreground = rgb(TEXT_2).into();
    theme.sidebar_accent = rgb(HOVER).into();
    theme.sidebar_accent_foreground = rgb(TEXT_STRONG).into();
    theme.sidebar_border = rgba(TRANSPARENT).into();
    theme.input = rgb(RAISED).into();
    theme.switch = rgb(RAISED).into();
    theme.switch_thumb = rgb(TEXT_STRONG).into();
    theme.scrollbar = rgba(TRANSPARENT).into();
    theme.scrollbar_thumb = rgb(FILL).into();
    theme.scrollbar_thumb_hover = rgb(HOVER).into();
}
