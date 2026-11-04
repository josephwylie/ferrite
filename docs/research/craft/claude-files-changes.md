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

- The Terminal pane: its tabs, head tools and menus.

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
