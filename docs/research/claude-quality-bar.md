# Claude desktop (Code tab): quality bar and feature list

Explored 2026-11-03, driven from a terminal session. A companion to `codex-quality-bar.md`. Not a spec to copy. It records what Claude ships and how well, so Ferrite can match the bar.

Skipped on purpose: Settings, voice, and Plan mode.

## New session (empty state)

- **Centred card stack:**
  - A small animated pixel mascot.
  - An environment card: "Default" and "Choose environment ⌄".
  - A suggestion list: "Import 2 Claude Code CLI sessions", "Turn on auto mode", "Choose a folder to work in". Each suggestion disappears once it no longer applies; picking a folder removes "Choose a folder".
- **Composer placeholder:** "Describe a task or ask a question".
- **Chips under the composer:** "This computer", folder, "Worktree" checkbox, model + effort, permission mode, and a usage ring.
- **Folder picker:**
  - A search field, then "No folder ✓", a Recent list and "Open folder…".
  - Picking a folder adds the Worktree checkbox chip.

## Composer

- **+ menu:**
  - Add files or photos ⌘U
  - Add folder
  - Import GitHub issue
  - Slash commands
  - Connectors ▸: Browse, Manage, and an inline toggle row for "Claude in Chrome"
  - Plugins ▸: one entry per plugin, each with its own submenu
- **Model menu:**
  - Header "Model · this session". Models have number shortcuts 1–4.
  - Effort ▸ submenu:
    - An explanatory line at the top.
    - Low / Medium **Recommended** / High / Extra / Max with a "⚠ 3.5× or more usage" pill.
    - An Ultracode toggle.
  - More models ▸.
  - A Fast mode section with a "Buy credits" pill.
- **Permission menu:**
  - Header "Permissions · this session". Each mode is a two-line row: name plus a one-line consequence. Auto: "Claude handles permission decisions".
  - Mid-session the menu adds number shortcuts 1–5 and **Recommended** / **Default** tags.
  - The chip is tinted orange when the mode is Manual.
- **Placeholder changes with state:**
  - Idle: "Type / for commands".
  - While a turn runs: "↵ to send, ⌘↵ to queue for later". The send button becomes a ■ stop button.
- **Slash picker:**
  - Opens on `/` above the composer, with "Type to filter" as a ghost hint.
  - A single column of names, fuzzy-filtered.
  - The highlighted row's description shows in a **side tooltip** next to the list, not inline.
  - Skills and commands share one list. There's no source label or icon (Codex does better here).
  - Picking an item inserts **blue `/name` text**, not a chip.
- **Usage ring:**
  - Hover: "Weekly · all models: 17% · Resets Tue 3:30 AM".
  - Click: a popover "Plan usage limits · Ultra", a progress bar, and "See all →".

## Transcript

- **Header line:** "Started on this computer · ferrite · Save as environment" (a link).
- **User bubble:**
  - Right-aligned, filled, radius ≈12.
  - Hover row: ✓✓, a relative time ("5 minutes ago"), copy, rewind ↺, branch. No tooltips.
- **Assistant reply hover row:** copy, branch, pin, read aloud, "Just now".
- **Tool calls:**
  - Rendered as **one prose summary line** per group: "Read theme.rs, ran 2 commands (1 stopped) (1 failed) ▸".
  - While working, the line describes the live step ("Fetching example.com status line ▸"), with a **shimmer** sweeping across the text.
  - Expanded, it becomes a bordered list of rows: "Read theme.rs ▸", "List a directory that doesn't exist **Failed** ▸", "Fetch example.com status line **Stopped** ▸".
  - Status words are coloured inline (Failed in red, Stopped in grey).
  - **Failed command expanded:** a `$ cmd` code well, then red "Exit code 1", then stderr in mono, all calm.
  - **Read row:** clicking it opens the file in the right panel.
  - **Long-running command:** "Waiting 40 seconds then printing done", plus a **"View live output ▸"** link.
  - Once done, the row switches to past tense: "Waited 40 seconds then printed done".
- **Todo list card:**
  - A bordered card of steps with ○ pending, spinner for active, ✓ done, and ⊖ blocked/denied.
  - It is re-planned in place when the user changes course: stale steps are replaced.
- **Live status line:** an animated dot glyph, then "9s · 87 tokens" or "3s · Thinking".
- **Rich content:**
  - Markdown tables with mono token chips.
  - File names are links ("theme.rs").
- **Scroll-to-bottom:** a round, translucent ↓ button floating above the composer.
- **Scrollbar:** a thin overlay. There is no minimap and no date dividers (Codex has both).

## Approvals (Manual mode)

- **Inline in the transcript, not replacing the composer.**
  - A pill row: an orange ◉ and "Needs your input".
  - A card titled "Allow Claude to run this command?", the plain-English intent, and a `$ command` well with syntax colours.
  - Deny, plus a split primary button.
- **The split button's default adapts:**
  - First ask: "Allow once ⌘↵", with ⌄ offering "Allow and switch to Auto — Auto reviews the rest of this session".
  - The next ask switches the default to "Always allow ⇧⌘↵", with ⌄ offering "Allow once ⌘↵" and "Allow and switch to Auto".
- **Sidebar:** the session row shows an orange dot while it needs input.
- **Deny ends the turn with no assistant text.** The summary line records "(1 stopped)", and the todo shows ⊖.

## Queue, steer, stop

- **⌘↵ while running queues.**
  - A strip appears above the composer: "**1 message queued** <preview>", then ↵ (send now), ✎ edit, × remove.
  - The strip fades in over about 100 ms.
- **↵ while running steers.**
  - The message joins the transcript at once as a pending bubble, with a **"Send now"** link under it. Clicking it forces an interrupt.
  - Otherwise it is delivered at the next tool boundary, and the model answers it in the same turn.
- **The queued message runs as the next turn** once the current one ends.
- **Background shell completion re-wakes the session:** a new turn starts on its own, e.g. "Showed command output and current time".
- **Stop (■):**
  - The turn ends silently. The summary line just says "Wait 25 seconds **Failed**".
  - The **todo card disappears** and there's no "You stopped after Ns" note.
  - Codex is clearly better here; Ferrite should do what Codex does.

## Attachments and images

- **⌘U opens the system file dialog.**
- **Composer:**
  - The composer grows and shows a ≈90 px rounded thumbnail. Hover shows an × badge on the corner.
  - Clicking the thumbnail opens a **lightbox**: a dimmed backdrop, the image centred, ×, and a **✎ annotate** button.
  - Annotate toolbar: pen, shape ▾, text, colour ▾, stroke ▾, undo, redo, clear, Save and ×.
- **Transcript:**
  - The sent image renders **large (≈360 pt wide) and right-aligned above the user bubble**, not as a small chip.

## Subagents

- **One row: "Ran 2 agents ▸".**
  - Expanded, one "Ran agent <title>" row per agent.
- **Clicking an agent opens it in the right panel:** ←, the agent's title, maximise and ×.
  - Inside: its prompt as a user bubble, "Model Haiku 5.5", its own tool summary rows and its result.

## Right panel and session rail

- **Title-bar icons** (top right):
  - Session rail
  - Terminal
  - Changes ⇧⌘D
  - Browser
  - Files
  - ⋯: Side chat ⌘;, iOS Simulator, and a "Keep computer awake (only for this session)" toggle
- **File viewer / editor:**
  - A tab strip; preview tabs are in *italic*.
  - A breadcrumb, line numbers, and a highlighted current line.
  - An optional file tree with a "Filter, or ? to search in files" field that reveals the current file.
  - A first-run coach mark: "Browse the project beside your file", with "Show file tree".
  - ⋯ menu: Show file tree ⇧⌘Y, File tree position, Preview tabs, Auto save, Tab key indents, Word wrap, Indent guides, Show whitespace, Scroll past end, Tab size.
- **Session rail (an inspector):**
  - Environment: computer with Save, folder and path, branch, and a Remote Control toggle.
  - Context: "25% used · 100.6k / 400k", a stacked colour bar, then per-category rows with tokens and %. The categories are Messages, System tools, MCP tools, System prompt, Skills, MCP instructions, Memory files, Since last update, Autocompact buffer, Free space, and the deferred tools. Rows for MCP tools and memory files expand.
  - Connectors ▸ "3 connected".
  - Changes: "+3,550 −0" and a **Create PR** button.
- **Text selection in the transcript** pops up "Start a side chat · Reply".

## Customize > Skills

- **Tabs:** Skills / Connectors / Plugins, plus a Yours / Discover segmented control.
  - Search, filter, sort, and "+ Add ⌄".
- **Discover:**
  - A coloured hero card ("From Anthropic · Data", Add).
  - Grids for "New skills" and "Most installed", each card with an icon, name, two-line description, source, install count and a + button.
- **Yours:**
  - Grouped "From Anthropic & Partners 32".
  - Each row: icon, name, plugin badge, "by X · description", date, and ⋯.
- **Detail:**
  - "← Your skills", a header ("by Matt Pocock · in Mattpocock skills · updated Oct 16"), and Overview / Contents · 3 tabs.
  - Overview: the description, plus a Created by / Plugin side card.
  - Contents: a version picker ("· current"), a file tree, and a rendered SKILL.md with a rendered/raw toggle.

## Motion (measured from a 120 fps screen recording)

- Popover menus (permission, model): open and close in **one frame (≈8 ms)**, with no animation.
- The queue strip appears over **≈100 ms**.
- Live work is shown with the shimmer on the active summary line and the animated dot glyph in the status line.

## Where Claude beats Codex (worth taking)

1. The prose summary line for tool groups, with inline coloured status words and a shimmer while live.
2. The split approval button whose default adapts, and the "switch to Auto" escape hatch.
3. ↵ = steer with a pending bubble and "Send now"; ⌘↵ = queue. The placeholder teaches this.
4. Context breakdown in the session rail, and the usage ring.
5. Image annotation in the composer lightbox.
6. Agent transcripts open in the side panel with the agent's own model shown.
7. The file viewer is a real editor: preview tabs, file tree and editor settings.

## Where Codex beats Claude

1. Stop and denied states: Codex says "You stopped after Ns" and keeps context. Claude goes silent and drops the todo card.
2. Skill picker: Codex shows a source label for each skill, chips and templates. Claude inserts plain blue text and keeps the description in a tooltip.
3. Long threads: Codex has date dividers and a tick minimap.
4. Tooltips on message action icons (Claude has none).
