# Codex desktop app: details observed by driving it (3 Nov)

## Transcript
- **Turn header** "Working for 7s" counts live, sitting over a full-width hairline. When the answer starts, it collapses to "Worked for 11s ›".
- **Plan** renders inline as checkboxes ("Run `echo hello`."), and ticks fill in as steps finish. There's no separate card.
- **"Thinking"** is plain text with a shimmer sweep and no dots.
- **Streaming** fades new tokens and rows in (newest row dimmer).
- **Code blocks** are rounded cards on a raised fill (~#2a2a2a). The header row holds a `</>` icon and the language name ("Plain text", "Diff"), with wrap-toggle and copy icons on the right.
- **Diffs** use red/green text only, with no line backgrounds.
- **Tables** have a header row with a hairline under it, roomy rows, and inline code as mono pill chips.
- **Inline code in the user bubble** also renders as pills.
- **File output** is a card: an app icon tile, the filename, the kind ("Excel"), and an "Open in ⌄" split on the right, with a "Give feedback" link under it.
- **Inline file links** show a small file-type icon, then blue link text.
- **Message actions** sit under the last answer: copy, fork, read aloud, ⋯.
- **Scroll-to-bottom** is a round ↓ button floating over the content above the composer.
- **Left turn-timeline:** a column of tick marks at the left edge of the transcript, one per turn, acting as a minimap.

## Composer
- **Environment tray:** a strip attached above the box: "Choose project" | "This computer".
- **Bottom row, left:** + then the permission chip ("Full access", in orange with an icon).
- **Bottom row, right:** a spinner while running, then "GPT-6 Astra Medium ⌄" (model plus effort in dimmer ink), mic, and a send button. Send turns into a voice waveform when the box is empty and a stop square while running.
- **Model popover:**
  - The effort level ("High") sits in blue at the top, with a "GPT-6 Astra ›" link under it that opens the model list.
  - Effort itself is a segmented slider with dots and a round thumb.
  - A reset icon sits at the right, a speed icon at the left.
- **Model list:**
  - It opens with a "Select model" header, then "Default — Recommended set of models", then the models.
  - A check marks the current model, and hovering a row shows a side tooltip ("Balanced agentic coding model").
- **+ menu:**
  - It spans the full composer width and sits right above the box.
  - Sections: "Add" holds Files and folders, Attach Finder, Goal, Plan mode, Record a skill and Sketch; "Plugins" holds Documents, PDF and Spreadsheets.
  - Each row has an icon, a label and a dim inline description.
- **Permission menu:**
  - The header is a question, "How should ChatGPT actions be approved?", with a "Learn more" link.
  - It has three options, each with an icon and a description line.
  - The current option is orange with a check.
- **Callout over the composer:** "Full access is on", with an explanation, "Learn more", a white pill "Don't show again" and ×.
- **Drafts persist per thread.**

## Shell, sidebar and header
- **Left icon rail:** Home, Space, Scheduled, Plugins, Explore, Code Review, Sites, with the profile avatar at the bottom.
- **Sidebar header:** a "Codex ⌄" switcher, a bell (it gets a dot badge when a turn finishes) and search.
- **Rows:** "New chat", then Projects grouped under folder icons, each group capped with "Show more", then Recents.
- **Row hover** shows pin and archive icons on the right.
- **A thread that finished while unseen** gets a blue dot at the right of its row, and the Home rail icon gets a blue badge. Both clear on view.
- **A new chat** appears in the sidebar immediately with a spinner. It is titled automatically a few seconds in ("Create UI demo and checklist"), and the header updates too.
- **Thread header:** a folder icon and the title on the left. On the right: ⋯, a checklist toggle and a new-window button.
  - **The ⋯ menu:** Rename ⌥⌘R, Pin ⌥⌘P | New side chat ⌥⌘S, Fork ›, Add scheduled task… | Share, Copy › | Open in new window, Open in › | Archive ⇧⌘A.
  - Every row has an icon and a right-aligned shortcut, with groups split by separators.
- **Loading a thread** shows a centred spinner, and the model chip shows a skeleton shimmer.
- **Empty home:** the logo mark, then "What should we build?", with the composer at the bottom.

## Ferrite gaps worth taking (ranked)
1. A "Working for Ns" header that collapses to "Worked for Ns ›" once the answer arrives. Ours shows a working line plus folded summaries.
2. Code blocks as cards with a language header plus copy and wrap.
3. Effort as a slider inside the model popover, with a model › link (this closes our effort-submenu item).
4. A thread header ⋯ menu with shortcuts: rename, pin, fork, copy, open in, archive.
5. File output cards with "Open in ⌄".
6. A blue unread dot (Codex) instead of our grey one, and a badge on the attention entry.
7. The + menu as a full-width sectioned panel with inline descriptions.
8. A permission menu with a question header and per-option descriptions. Ours has notes already; align the header and colours.
9. A turn-timeline minimap in the left gutter.
10. Auto-title appearing live in the sidebar and header, with the new row's spinner.

## Deeper pass (3 Nov, 12:00)
### Transcript
- "Worked for 11s ›" expands to the turn's commentary, the plan (unticked at that point) and a "Ran commands ⌄" group. The group nests one row per command ("Ran echo hello", "Ran ls … ›"), and each of those expands to a Shell card: a "Shell" label, `$ cmd`, then the output. The card has a max height, its own scroll and a bottom fade mask.
- While live there's one activity row that swaps in place, in the present tense with an icon: "Reading theme.rs" → "Searching files in theme.rs folder" → "Read files". Then "Running curl … | head -1".
- Tables put a colour swatch next to each hex value. A file reference renders as "From ◉ theme.rs" (an icon plus blue link).
- A new turn scrolls the user's message to the top, leaving empty space below for the answer.
- "Starting your chat…" is a shimmer placeholder, and the composer is disabled with that as its placeholder text.
- Under the answer: copy · rate · read aloud · ⋯ (Branch in new chat, Hook stats), with a time ("12:03") on hover.
- Hovering the user bubble shows the time, copy and an edit pencil. Edit turns the bubble into a full-width editor with Cancel/Send, and URLs become link tokens with a globe icon.
- When idle, the composer shows a suggested follow-up as ghost text with a "Press Tab to accept" chip.
- The composer renders markdown live: ``` becomes a code block inside the box with its own horizontal scroller.

### Approval
- The approval card replaces the composer. It shows a "Terminal" source label, a question ("May I access https://… to fetch …?") and the command in a mono well. Below that: "Deny Esc" and a split button, "Allow once ↵ | ⌄".
- The ⌄ menu offers Allow once and Allow similar commands ⓘ. The ⓘ tooltip says it allows commands that start with <prefix> for this conversation.
- The sidebar row gets a green "Needs approval" pill, and the bell gets a dot. "Working for Ns" keeps counting during the wait.
- Turning on Full access opens a confirm dialog: ⚠ title, a grouped list of capabilities (each with an icon, title and description), risk copy, then Cancel and a red-tinted Confirm.

### Sidebar row
- Hover shows Pin and Archive icons. Project headers get "New chat in <project>" on hover.
- Right-click menu: Rename ⌥⌘R, Pin ⌥⌘P, Mark as unread ⇧⌘U, Section ›, Fork ›, Share, Copy ›, Open in new window, Open in ›, Archive ⇧⌘A, Permanently delete.
- The switcher has ChatGPT ("Create, learn, and explore") and Codex ("Build, debug, and ship") ✓, each with a subtitle.

### Thread ⋯ submenus
- Fork ›: Fork chat, Fork chat in new worktree.
- Copy ›: Copy deeplink ⌥⌘L, Copy as Markdown, Copy working directory ⇧⌘C.
- Open in ›: Finder, Terminal, Xcode, each with its app icon.
- "Add scheduled task…" opens a panel tab with a prompt box and grouped cards: Details (Runs in ⌄), then Frequency (Repeat: Interval ⌄, Every 30 minutes ⌄, Notifications: Important updates ⌄). The footer has Cancel and "Create scheduled task".

### Right panel: a tabbed workspace (the biggest gap)
- "New side chat" opens a right panel with browser-like tabs: icon, title, × on the active tab, and +. Side chats are temporary; the empty state reads "Side chats are temporary and disappear when you close the app."
- The "+" new tab is a launcher. It has back/forward/reload, a "Search or enter a URL" bar and a Tools grid with shortcuts: Changes ^⇧G, Terminal ^`, Files ⌘P, Side chat ⌥⌘S, New page. Below that, Suggested sites with favicons. Tools that are already open drop out of the grid.
- **Changes tab:**
  - Header: a scope dropdown ("Branch +3,551 −0"), then main → origin/main.
  - Scope menu: Last Turn | Uncommitted, Unstaged, Staged | Committed ›, Branch ✓.
  - Toolbar: Changes options ⋯ (Load full files, Rich preview, Word diffs, Hide whitespace, Hide imports, Copy git apply command), Jump to file, Refresh, Word wrap, Collapse all diffs, Switch to Auto diff, Hide files.
  - On the right, a file tree with a filter, file-type icons, +N counts and a stage (+) button per file.
  - On the left, one continuous diff of every file. Clicking a file in the tree scrolls to it. The diff is syntax-highlighted with a green gutter bar, and each file header shows +N −N, view, open and ⋯.
  - Empty state: an icon, "No file changes yet", "The latest diffs are no longer available." and a "View branch diff" button.
- **Terminal tab:** a real PTY in the project directory, with the tab named after the project. The shell starts lazily, so input typed early shows up before the prompt.
- **Files tab:**
  - The tree has three sections: All changes, Changed files and All files. Its filter matches fuzzily and shows only the matching paths.
  - Viewer: skeleton lines while loading, a breadcrumb path, line numbers and soft wrap.
  - "Show changes" toggles an inline diff against Branch or origin/main.
  - The "Open ⌄" split button lists Default app, Terminal and Xcode, then Open in folder, Open in GitHub and Save as….
- **Maximise:** the panel takes over the main area and the chat becomes the first tab. A compact floating composer stays docked at the bottom, with the model and permission shrunk to icons.

### Other pages
- Code Review: a full-bleed page with the sidebar hidden and an animated glyph-rain background. It shows an icon tile, "Code review", a subtitle, a white "Set up GitHub" pill and a secondary GitLab button.
- Space:
  - Its own nav: Create, All, Pages, Slides, Sites, Images, then Recents and Spaces, with Trash pinned at the bottom.
  - All is a table (Name, Source, Last activity) with segmented filters (Suggested, Favorites, Your items, Shared with you), a list/grid toggle and a ⋯ on row hover.
  - The Create ⌄ menu: Page, Slides, Sheet (Coming soon), Site, Image | Folder, Space | Start from template, Upload files, Connect a plugin.

## Claude app: tool-call rendering (from your screenshots)
- Consecutive tool calls group into one collapsible card of rows ("Captured window in X ▸"), each with a thumbnail on the right.
- Hovering a thumbnail opens a preview popover with the image and the caption "Captured window in <app> · 1 of 3", with arrows to page through.

## Updated ranking for Ferrite
1. Right-panel workspace with tabs: Changes (scoped diff and tree), Terminal, Files, and a side chat. This is the biggest structural gap.
2. "Worked for Ns ›" that expands to commentary and a nested "Ran commands" group with per-command shell cards (max height, fade).
3. Approval card that replaces the composer, a split "Allow once ↵ ⌄" with "Allow similar", and a "Needs approval" pill on the sidebar row.
4. Code-block cards (language header, wrap, copy) plus inline colour swatches and file-link chips.
5. Grouped tool-call rows with thumbnails and a hover carousel (Claude's pattern) for screenshots and images.
6. User-message edit in place; an action row with time on hover; ⋯ with Branch in new chat.
7. Ghost suggested follow-up with Tab to accept.
8. Thread ⋯ and right-click menus with full shortcuts and submenus (Fork in worktree, Copy deeplink/Markdown/cwd).
9. Live single activity row in the present tense.
10. Effort slider and model link in the model popover.

## Subagents and images (3 Nov, 12:09)
- **In the transcript**, a spawn shows as a single line of overlapping coloured avatars, each agent with its own glyph and colour (green, yellow, blue): "Agent a, Agent b and Agent c started working". Later the line updates to "Agent b and Agent c finished".
- **Clicking that line** opens a right-panel "Subagents" tab.
  - The list is grouped into "Active · N" (avatar, name, "Working", elapsed time) and "Done · N" ("just now"). When nothing is running it reads "No active subagents".
  - A row opens that agent's own transcript, with a ← back arrow, the name and avatar, and a model badge ("GPT-6 Astra · Medium"). The transcript has the same "Working for 13s", the live command row, "Worked for 14s ›", the answer and an action row.
- **The parent turn** waits as "Thinking", then summarises each agent in a bulleted list.
- **Viewing images:**
  - The tool row reads "Viewed 2 images ⌄" and expands to small rounded thumbnail tiles.
  - The final answer embeds the images inline as rounded cards at modest size.
- **Image lightbox:**
  - Clicking an image dims the whole app. The image scales up from its spot in the page.
  - The top bar has "Image ⌄" on the left, which holds the image title (alt text, "Navigation reference"). On the right: a zoom menu (25/50/100/150/200%, Zoom to fit ✓), download and ×.
  - ← and → arrows sit at the edges, with a "1 of 2" counter at the bottom. Esc closes it.
- **Not tested:** dropping in a file attachment, because the native picker is filtered under computer use. What I did see: an image attached in an older thread shows as a thumbnail above the user bubble.

## Quality-bar pass (3 Nov, 12:15–12:25): measurements and flows
### Measured at 1920×1080, 1×, so px = pt
- **Surfaces:**
  - rail and title bar #212121
  - sidebar #1b1b1b
  - main area #171717
  - composer #2f2f2f
  - selected sidebar row #2d2d2d
  - 1px hairlines between regions, #272727–#343434
  - overlay scrollbar thumb #2e2e2e, about 6px wide
- **Transcript column:** 736pt wide, centred. The composer is the same width, so their edges line up exactly.
- **Body text:**
  - about 14pt, line height 23pt (≈1.6)
  - paragraph gap about 14pt; list items on a 23pt pitch
  - bold lead-ins ("**Yes—there was a bug.**") used as a summary opener
- **User bubble:** right-aligned, at most about 70% of the column, radius about 16, padding about 16/12, fill #2f2f2f-ish.
- **Composer:** about 98pt tall at rest, radius about 20. The placeholder "Work with Codex" sits top-left. Bottom row: + · ⓘ Full access (orange) … spinner · model + effort (dim) ⌄ · mic · a round white send (a waveform when the box is empty).
- **"Worked for 20m 51s ›":** dim, with a full-width hairline under it.
- **Timestamps:** when a later session resumes the thread, a centred dim divider "Mon 19 Oct at 17:10" appears.
- **Turn minimap:** about 40 small ticks stacked at the far-left edge of the main area, one per turn. The visible range is drawn brighter.
- **Scroll-to-bottom:** a round ↓ button about 30pt across, floating centred just above the composer and overlapping the text.
- **A failed attachment** shows as a square tile with a red hairline border and red "Failed" text, above the bubble.

### Motion (120fps screen recording)
- The model/effort popover fades and blurs in over about 100ms (soft-focus to sharp), with no slide. The chip's label swaps instantly to "Select effort".
- The permission menu appears within one frame (≤16ms), with no animation. Fast menus snap; richer popovers blur in.
- Switching threads clears the main area instantly, with no crossfade. Skeleton bars stand in for the composer chips until the thread loads, then the content appears all at once, scrolled to the bottom.

### Skills
- Typing `$` opens a skill picker anchored to the composer that grows upward, with the nearest match at the bottom. Each row: icon · name · dim description · source on the right ("From Spreadsheets", "Local file", "Built-in", "Sites"). There's a scrollbar, and typing filters fuzzily.
- Choosing one inserts a blue skill chip with an icon. If the skill ships templates, a "Templates" carousel opens above the composer: preview cards, "Create template", ‹ › and ×.
- **The Customize page** (rail):
  - Navigation: Plugins, Skills, then Installed (each with an icon).
  - Skills: Installed in a 2-column grid (icon, name, one-line description, ✓), with "See X and 170 more" and source tabs (agenticOS / Personal / System).
  - Plugins: Popular, New & Noteworthy and Small Business sections, each row with a logo tile, name, description and +.
- Skill detail is a modal: icon, enable toggle, ⋯, ×, title plus a "Skill" tag, the rendered SKILL.md in a well, a red "Uninstall" and "Try now".

### Errors and failures
- **A failed command** gets no red chrome. The prose says "`ls …` exited with code 1:" and stderr shows in a plain code card. Tone stays calm.
- **Stopping the turn:** the header becomes "You stopped after Ns", and the running tool row turns into "■ Background terminal stopped with sleep 40" (later just "Ran sleep 40"). There's no banner or modal, the composer comes back at once, and the stop button reverts to send.

### Sending while a turn is running
- The composer stays fully usable. Enter queues the message: it becomes a strip above the composer showing an icon, the text, then "↪ Steer", 🗑 and ⋯ (Edit message / Open in side chat / Turn off queueing).
- **Steer** interrupts the running tool and places the message inline in the same turn as a right-aligned bubble. The same "Working" block carries on.
- Claude, for comparison, puts a placeholder in its composer: "↵ to send, ⌘↵ to queue for later".

### Attachments
- The + menu has Files and folders, which opens the native picker, and "Attach <frontmost app>".
- **In the composer:**
  - An image becomes a large (about 90pt) rounded thumbnail, with a × on hover in the top-right.
  - A text file becomes a card with a rendered first-page preview and a footer bar showing the file icon and name.
  - The composer grows to fit the tray.
- **In the transcript:**
  - Above the bubble and right-aligned: a small square image thumbnail, and the file as a compact pill (📄 README.md).
  - Clicking the pill opens the file in the default app (it launched Xcode).
- **Sketch** (in the + menu) is an inline drawing canvas: select, pen, text, shape and eraser tools, undo/redo, 14 colour swatches, a brush-size slider and a ✓ to attach.

### Long threads
- Turns from different days are separated by date dividers, and the left-edge minimap scales with the turn count.
- Long work summaries collapse to "Worked for 20m 51s ›".
- Opening one jumps straight to the end. Old content loads above as you scroll, with no visible pagination spinner.
