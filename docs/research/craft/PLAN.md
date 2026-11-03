# Craft plan: parallel lanes, tests at checkpoints

Sources: `geometry.md`, `typecolour.md`, `components.md`, `motion-measured.md` (all in this folder). Each row there has the target value and its evidence; lanes implement those rows and cite them. Where Claude and Codex disagree, the docs pick one; this plan follows those picks.

## Operating rules (every agent)

- **One worktree, one target dir.** Every lane edits `transcript-app-style` directly, on disjoint files. No per-agent worktrees (each costs GBs). Shared `CARGO_TARGET_DIR=~/.cache/codingos-target` (4.5 GB now).
- **No tests in lanes.** A lane may run `cargo check -p <its crate>` at most twice: once midway, once at the end. No `cargo test`, no `--visual-reference`, no clippy. If check fails in a file another lane owns, note it and stop; don't fix it.
- **No commits in lanes.** The orchestrator commits per lane at the checkpoint, by the lane's file list.
- **All 8 lanes run at once after checkpoint 0** (files are disjoint; cargo serialises checks through the shared target lock). Disk guard: the orchestrator checks `df` before each wave and stops below 10 GB free.
- **Nothing touches the running app.** No install, no launching Ferrite, no killing `ferrite server`.
- **Tests that pin old copy or values** are updated by the lane that changed them, in the same files, but run only at the checkpoint.
- Never `rm` through a shell variable.

## Checkpoint protocol (orchestrator only)

1. `cargo check --workspace`, then `cargo test --workspace` once.
2. Failures go back to the owning lane agent (SendMessage, same context) for one fix round. Re-run only the failing crates' tests.
3. Render the visual reference once (`FERRITE_REF_SIZES=app`) into `craft/ferrite-cpN/` and compare with `claude/` and `codex/`.
4. Commit one commit per lane. Push and open/refresh the PR only at the last checkpoint.

## Wave 0: groundwork (serial, one agent, then checkpoint 0)

The refactor-first rule: almost every lane touches `pane.rs` (10.5k lines) and `theme.rs`, so split and settle them before fanning out.

1. **Move-only split of `pane.rs`** into `pane/working.rs` (working line, todo), `pane/composer.rs` (box, placeholder, footer chips, send, queue), `pane/menus.rs` (menu popover/rows, picker rows, footer), `pane/output.rs` (output box, command fold, failed excerpt, diff), `pane/usage.rs` (context usage, ring, ink). No behaviour change.
2. **Merge the two `ChoiceMenu` types** (`ferrite-ui/src/menu.rs:535`, `components.rs:1833`) into one.
3. **Token pass in `theme.rs`** (all values from `typecolour.md` / `geometry.md` / `motion-measured.md`):
   - Type: prose 14/21, chrome 14, meta 13; weight 600 for bold, table heads and headings.
   - Surfaces: composer and menus #303030 with #404040 / #383838 edges; selection nav #2a2a2a, menu cursor #3e3e3e; code fence #262626 / #333333; scrollbar thumb #5e5e5e.
   - Lines: collapse `BORDER`/`LINE`/`LINE2` + six aliases to two greys (#303030 seams, #222222 card borders).
   - Text: primary #efefef, meta #9c9c9c, placeholder #878787; error #f3767b; risky-mode amber instead of yellow.
   - Radii: eight tokens to three (6, 10, 12).
   - Motion: `MOTION_MENU_IN_MS` 0; caret 1.0 s hard blink; braille frame 66 ms; one hover duration.
   - Replace legacy ink aliases with the documented tier names; fix the `nav.rs:1801` hover blend bug; fix theme rule 8 and the wrong 13/18 comment.
4. Delete dead code: the star spinner (`working_spinner`, components.rs:542, and its gallery use).

**Checkpoint 0:** full check and test, visual reference, one commit per step. Everything after this assumes the split.

## Wave 1: lanes A–H in parallel (one wave), then the final checkpoint and PR

| Lane | Work (rows in the docs) | Files it owns |
|---|---|---|
| **A. Chrome + nav** | 44 title bar with hairline; hide the subject strip with one tab; delete the bottom status bar; fix `x · x` title bug; title ▾ menu; sidebar 256 wide, row pitch 32, r 6, thread title aligned at ≈35; the tree no longer carries Working/Needs you/Done sections; status shows as the row's right mark (spinner, amber dot, red dot). **Activity view (owner's call, Codex-style):** a bell button in the sidebar header swaps the tree for an activity list with a "Priority" section (running or needs you) and a "Today" section. Each row shows the title, then the project in grey, then a 2-line grey preview of the last message, with the spinner at its right. The bell is filled while the view is on. Bottom status bar removed; the Solo/view switcher moves to the sidebar foot. | `cockpit/view_head.rs`, `nav.rs`, cockpit.rs nav range (≈9250–9700, 13365–14312), status bar file |
| **B. Transcript rows** | narration at full ink; message actions (copy, fork, ⋯; always on the final answer); relative time; paragraph gap 12, list +3, bubble→work 40, card→prose 22; bubble r 10, max 84%; column 736; tables full width 40/36 rows; paths as plain chips; jump-to-latest snap; streaming 300 ms per-run fade, bubble 250 ms, no rise, one frame source | `transcript.rs`, `transcript/rows.rs`, `rich.rs` |
| **C. Tool groups** | Claude-style summaries (one grey, "(1 failed)", max two kinds); rows lead with description, strip `cd … &&`, no previews on shut rows except test verdict; red `Failed` tag; Claude hairline card (r 10, rows 36); shell card (label, `$ cmd`, status); snap group, folds, chevron | `pane/text.rs`, `pane/output.rs` |
| **D. Live state** | one working line (glyph + current call + elapsed; verbs only while thinking); drop "esc to interrupt" and the tab-bar task meter; todo current mark = the arc spinner; single error strip; muted "Interrupted"; delete the thread-switch veil | `pane/working.rs`, `ferrite-core/src/progress.rs`, `pane.rs` remainder (`failure_strip`, `tab_strip`), cockpit.rs veil lines (≈5841) |

### Lanes E–H (same wave)

| Lane | Work | Files it owns |
|---|---|---|
| **E. Composer** | Claude's placeholders; queue strip as its own card (44, r 10, 8 above), one row per message, send-now / edit / remove; composer 16 wider than the column, 44 tall, 41 to window bottom; model+effort as one chip, drop `Default ·`; empty-thread environment chips; ring shows plan usage when reported | `pane/composer.rs`, `pane/usage.rs`, `background_chips.rs` |
| **F. Menus** | compact model menu (≈260 wide, effort as a submenu, no descriptions, no key-hint footer); permission `Default`→`Manual`, colour only risky modes; slash list with inline descriptions, skills folded in with a tag; menus r 12; remove the blank first frame | `pane/menus.rs`, `ferrite-ui/src/menu.rs`, cockpit.rs menu range (≈8128–8860, 11904–12900) |
| **G. Approval** | 480 wide; primary `Allow once ⌘↵`, `Always allow <pattern>` in the split; drop the explanation line and the question card's key-hint footer; normal placeholder; 150 ms fade-in | `decision.rs` |
| **H. Side panel + empty state** | one scroll of sections (Environment, Changes, Context, Connectors), no tabs; PR as a Changes row; one context meter; 320 wide, padding 16; panel snaps, column fixed; centred empty-thread card | `cockpit/panel.rs`, `cockpit/panel/details.rs`, `pane.rs` `render_draft` area, `cockpit/empty_board.rs` |

## Not in scope

Mic/dictation, Codex's plugin catalogue, the Codex "Worked for" header, rewind (needs core support), toasts and hover motion (not recorded).

## Cost

10 implementation agents: 2 serial in Wave 0, then 8 in parallel. Two full test runs in total: checkpoint 0 and the final checkpoint.

## Wave 2: features (owner's picks, 2026-11-03), after transcript step 2

Same quality bar as Wave 1: measured against the captures, tokens only, snaps per motion-measured.md.

| Lane | Work |
|---|---|
| **N. Navigation** | Back/forward arrows in the title bar moving through UI history (thread and view changes), ⌘[ / ⌘], tooltips "Back ⌘[" / "Forward ⌘]", disabled at the ends. Title-bar tool buttons (Claude top right) at the same quality. |
| **S. Sidebar** | Codex bell = activity filter that replaces Inbox (Priority / Today, as Lane A built); search icon in the sidebar header; Pinned section (pin/unpin from the row menu); collapsible project groups (▾); "Show more" after ~5 threads per project; Recents section. |
| **P. Side panel** | Create PR in Changes; Context breakdown that opens (MCP tools, skills, memory files lists, deferred tools apart); Connectors collapsible with "N connected"; Environment Save (if confirmed); usage popover from the ring. No Remote Control. |
| **T. Composer + transcript** | + menu Connectors › and Plugins › submenus; Codex home hero "What should we build in <project>?" with project switcher and env chips above the composer; image attachments as thumbnails in the user bubble; quiet "N background agents completed" line; floating scroll-to-bottom ↓ button. |
| **R. Edit + rewind** (core + UI) | Edit a sent prompt (Codex) and rewind to a message (Claude): truncate the session at that user message and resend/continue from there. Needs core support per provider (Claude CLI resume-at / fork-session; Codex thread rollback), a protocol op, transcript truncation, and message actions on the user bubble. Research the provider mechanisms first, then build. |

Lane R design: docs/research/edit-rewind.md (Claude `rewind_conversation` + `rewind_files`, fallback `--resume-session-at`; Codex `thread/revert` / `thread/fork beforeTurnId`; store `Rewind` record schema 15; `Op::Rewind`/`Op::Fork`; prompt uuids on send). Starts with a capture test pinning `rewind_conversation`.
