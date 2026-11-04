# Claude Desktop: title-bar tools, Files explorer, Changes, session rail

Captured live from Claude Desktop (Code tab) on 2026-11-04. Screenshots are in
`claude/parity/`. This is the parity spec for Ferrite's right-hand tools, and it
replaces Ferrite's "Show in Finder" title-bar button.

## Title-bar tools (left to right, at the right of the title bar)

| Icon | Tooltip | Shortcut | Opens |
|---|---|---|---|
| ▤ | Show session rail | — | the session rail card (below) |
| >_ | Terminal | — | an in-app terminal in the thread's folder (**in scope**; capture its pane before building) |
| ⊞± | Changes | ⇧⌘D | the Changes view |
| 🌐 | Browser | ⇧⌘B | the in-app browser (out of scope here) |
| 📁 | Files | ⇧⌘F | the Files explorer |
| ⋯ | more | — | not captured |

The active tool's icon shows a pressed ground.

## Session rail (`session-rail.png`)

A floating card dropped from the ▤ icon, about 210 pt wide, over the transcript.

- **Environment:**
  - `This computer` with a `Save` link at the right;
  - `ferrite · ~/Desktop/Projects`;
  - the branch (`main`);
  - `Remote Control` with a toggle (skipped for Ferrite).
- **Context:**
  - `27% used` and `109.9k / 400k`;
  - a segmented bar;
  - rows of colour swatch, name, tokens and %: Messages, System tools, MCP tools, System prompt, Skills, MCP server instructions, Memory files, Autocompact buffer, Free space;
  - then the deferred tools, MCP tools (deferred) and System tools (deferred), shown with "—";
  - then `▸ MCP tools 197` and `▸ Memory files 2`, which expand.
- **Connectors:** `Connectors ▸` with `3 connected`.
- **Changes:** a `+3,800 −0` diffstat, then a `Create PR` button.

**Correction (`terminal-tabs.png`, 2026-11-04):** the rail is not a floating card. With a side chat and the Terminal open it is a docked column in the right-hand area, the transcript reflowing narrower: `[transcript][rail][side chat][terminal]`. Ferrite keeps its existing panel column as the rail, and the title-bar door toggles it.

The owner asked what `Save` does. Its tooltip didn't show, so this is still unconfirmed.

## Files explorer (`files-empty.png`, `files-code.png`, `files-more-menu.png`)

It opens as a right-hand area with two columns: an editor on the left and the file tree on the right. The transcript narrows to its left.

- **Head.** The editor's tab strip on the left (`motion.rs ×`; italic means a preview tab). At the right:
  - 🔍 `Go to file or search` ⌘P;
  - `Collapse all folders`;
  - ☰ `Hide file tree` ⇧⌘Y;
  - ⋯;
  - expand ⤢;
  - close ×.
- **Tree:**
  - a filter field at the top: `Filter, or ? to search in files`;
  - folders with ▸/▾ and files with type icons (`<>` for code);
  - git status colours names (`.worktrees` green with a green dot);
  - the selected file's row is highlighted.
- **Empty editor.** A folder icon over `Open files appear here`, then `Pick a file in the tree, or click a file path in the conversation.`
- **Editor:**
  - a breadcrumb path (`crates / ferrite-ui / src / motion.rs`);
  - line numbers, with the current line highlighted;
  - mono text, soft-wrapped;
  - it is an **editor**, not just a viewer: `Auto save` is on.
- **Right-clicking a file in the tree:**
  - `Attach as context`;
  - `Copy ›` (`Absolute path`, `Relative path`, `Filename`);
  - `Show in Finder`.
- **⋯ menu:**

  | Item | Default |
  |---|---|
  | Show file tree | ✓ ⇧⌘Y |
  | File tree position › | Automatic |
  | Collapse all folders | — |
  | Preview tabs | ✓ |
  | Auto save | ✓ |
  | Tab key indents | ✓ |
  | Word wrap | ✓ |
  | Indent guides | ✓ |
  | Show whitespace | off |
  | Scroll past end | off |
  | Tab size › | 4 |

  The menu is grouped in four blocks:
  1. Show file tree, File tree position, Collapse all folders;
  2. Preview tabs, Auto save, Tab key indents;
  3. Word wrap, Indent guides, Show whitespace, Scroll past end;
  4. Tab size.

## Changes (`changes-split.png`, `changes-more-menu.png`)

⇧⌘D opens Changes as a split **under** the Files editor (or alone, when Files is closed).

- **Head:**
  - a scope dropdown, `Uncommitted changes ▾`, holding:
    - `All changes` (vs main);
    - `Uncommitted changes` ✓;
    - `Compare against › main`;
  - at the right: 🔍 search, ☰ file list, ⋯, expand ⤢ and close ×.
- **Body:** one block per file, in a continuous scroll.
  - The file head reads `▾ apple-gpu-performance.md docs/research`, with `+126`.
  - It also has a ✓-circle (mark as reviewed), ↗ (open the file) and ⋯; the ⋯ menu wasn't captured.
  - Then the diff, with line numbers. Added lines are on a green ground with a green left bar.
- **⋯ menu:**

  | Item | Default |
  |---|---|
  | Show files | ⇧⌘Y |
  | File list position › | Automatic |
  | Group files by folder | — |
  | Separate test, build, and generated … | — |
  | Collapse all files | — |
  | Expand all files | — |
  | Side by side | — |
  | Word wrap | ✓ |
  | Highlight changed words | ✓ |
  | Hide whitespace changes | — |
  | Refresh | — |

## Composer + menu (`plus-menu-open.png`)

The menu:
- `Add files or photos` ⌘U
- `Add folder`
- `Slash commands`
- `Connectors ›`: `Browse connectors`, `Manage connectors`
- `Plugins ›`: a submenu per plugin (`Marketing skills ›`, `Imessage ›`, `Mattpocock skills ›`), then `Manage plugins` and `Browse plugins`

## Owner scope (2026-11-04)

- The terminal is **in**.
- Read aloud is **out**.

## Still to capture

- The ⋯ menu on a file in Changes.
- The file-list view in Changes.
- The title bar's ⋯ menu.
- A right-click on a link in the transcript.
- What `Save` on Environment does.
- A Markdown file and a `.txt` file in the Files editor, for the font and rendering.

## Background tasks (`bg-tasks-panel.png`, captured 2026-11-04)

**In the transcript:**
- the launch row reads `Running sleep test command ▸`, in muted ink;
- on completion, a muted single line appears at that point: `Background command completed · Sleep 30 seconds then echo agent-done`. For an agent, the format is `Background task completed · Agent "…" · took 22m 55s`.

**In the session rail:** a `Background tasks` section listing each running task with its title, a live elapsed time and a white stop square, then `1 finished`.

**The Background tasks pane** opens from the rail into the right-docked pane. Its head is `Background tasks`, with trash (clear finished), expand and close.
- **`Running N`**: a card per task, with its title and `›`, the elapsed time and a stop button. Inside, a `▾ 1 task` tree; nested children are grouped under `▸ 1 finished`. Each child is `▾ <title>` with its duration, then its output block in mono: `$ command`, the output lines, and `[exited with code 0]`.
- **`Finished N`**: collapsed rows, each a `▸ title` with the duration at the right (`Sleep 20s then print done  20s`).
- **Owner note:** while a task runs, its transcript row is **live** (it animates and updates). When it finishes or stops, it turns back into plain muted text. The live look isn't captured yet; capture it on the next run.
- There is no pill above the composer. Running work lives in the rail and the pane.

### Owner's captures, 2026-11-04 (`bg-sidechat-midturn.png`, `bg-live-row.png`, `bg-pane-running.png`, `bg-card-collapsed.png`, `bg-card-expanded.png`)

- **Live transcript row** while a task runs: `Waiting 45 seconds in the background` in muted ink, then `View live output ▸` in brighter ink. Clicking the link opens the Background tasks pane. When the task ends, the row settles to plain muted text.
- **Session rail:** `Background tasks`, then one row per task: a terminal icon, the title truncated with "…", a live elapsed time (`15s`) and a white stop square.
- **Pane card:**
  - a `▾`/`▸` disclosure, the title, the elapsed time at the right (`33s`) and a stop button on a raised square;
  - expanded, it shows mono `$ sleep 45`, then the output, or `No output yet` in muted ink;
  - a focused card has a blue focus ring.
- **Side chat mid-turn: Claude allows it at any time, even while text is streaming.**
  - The fork is taken at the *current* point, so it includes the running turn.
  - A background shell running at fork time is reported in the side chat as `Background task stopped · Background shell command didn't finish before …`.
  - The tab takes its name from the first message (`testing`).
  - After the first message, the composer placeholder becomes `Reply in this session…`.

## Terminal (`terminal-open.png`, `terminal-command.png`, `terminal-selection.png`, `terminal-menu.png`, `terminal-tabs.png`, `terminal-more-menu.png`, `terminal-attach-*.png`; captured 2026-11-04)

- **Door:** `>_` in the title-bar tools, tooltip `Terminal ⌘J`; the pressed ground shows while the pane is open.
- **Placement:** its own column, right-docked at the far right, about 417 pt wide, full height under the title bar. The other columns (transcript, rail, side chat) stay, narrower. Same `#181818` ground as the rest, a 1px divider at its left.
- **Head:** about 52 pt tall, no rule under it. At the left the tab label `Terminal` and a `+` (new tab); at the right `⋯`, expand, close. A single tab is bare text with no x. With a second tab the names are `Terminal 1` and `Terminal 2`, each with a trailing x; the active tab is a filled pill (`#3a3a3a`), the inactive one bare grey text, and the `+` moves after the last tab. The x kills that tab's shell; back to one tab it is bare text again.
- **Body:** the user's login shell in the thread's folder, the prompt as the shell draws it. Mono, about 11 pt with 15 pt line height, off-white; about 10 pt padding at the left, 6 pt at the top; wraps at the pane width. Block cursor, blue (`#1a8cff`) focused and a hollow outline unfocused, blinking. A small dot in the left gutter marks a command line (shell integration).
- **Selection:** a flat blue-teal band (`#113852`) in full-width rows, light text.
- **Right-click menu** (about 202 pt wide): `Attach selection as context ⇧⌘L`, rule, `Copy ⌘C`, `Paste ⌘V`, `Select all ⌘A`, rule, `Clear terminal`. With no selection `Attach selection as context` is absent (not disabled) and `Copy` is greyed.
- **Attach as context:** a selection also shows a small dark `Attach as context` pill floating directly above the selection, centred on it, label only (no icon, no shortcut, no tooltip). Pill and menu row do the same thing: they attach the selection only (not the visible output) as a tile in the composer tray: the attachment tile (about 120 pt square, rounded, bordered) with the selected text in small grey type behind a thin vertical quote bar, newlines read as spaces, an outlined `Excerpt` badge at its bottom-left, and the corner x on hover.
- **`⋯` menu:** `Cursor style ›` (Block), `Blink cursor` ✓, `Copy on select`, `Scrollback ›` (10,000 lines).
- **Not captured:** tooltips for `+`, `⋯`, expand and close; the Cursor style and Scrollback submenus.
