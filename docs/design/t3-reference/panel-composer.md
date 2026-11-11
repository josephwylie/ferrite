# T3 Code vs Ferrite: right panel, header breadcrumb, composer, model picker

Paths: T3 = pingdotgg/t3code at `bcee0f5d`, `apps/web/src`. Ferrite = `crates`.
Tailwind scale: 1 unit = 4px. T3 dark tokens (index.css:1192+): background #000, foreground neutral-100,
muted-foreground neutral-400, accent white/4%, border white/8%, popover neutral-950 mixed 3% white,
primary oklch(0.571 0.21 264) (blue). --radius 0.5rem (8px), --control-radius 0.5rem, radius-sm 4, radius-md 6.
--workspace-topbar-height 52px (min 40 under WCO).

---------------------------------------------------------------------------------------------------
## 1. T3 right panel

### 1.1 Data model: one union, one list, one active id (rightPanelStore.ts)
- `RightPanelSurface` (rightPanelStore.ts:37-79) is a discriminated union. 9 kinds
  (`RIGHT_PANEL_KINDS`, :15-25): diff, files, file, preview (browser), device, terminal, pull-request,
  pull-requests, side-chat.
- The id IS the dedupe key, kind-prefixed:
  - singletons: `"diff"`, `"files"`, `"pull-requests"`, `"device"` (or `device:<x>`)
  - per resource: `browser:<tabId>` (`browser:new` placeholder), `terminal:<id>` (carries
    `terminalIds[]`, `activeTerminalId`, `splitDirection` — splits live INSIDE one tab),
    `file:<relPath>` / `attachment:<id>` (+ revealLine, revealRequestId, folderTrail),
    `pull-request:<host/repo#n>` (+ `detailTab`: summary|guide|timeline|code),
    `side-chat:<threadId>`.
- Per thread state (:91-98): `{ isOpen, activeSurfaceId, surfaces[], maximized?, dismissedDeviceSurfaceIds?, proactiveDiffTurnId? }`
  keyed by `scopedThreadKey(threadRef)` in `byThreadKey`.
- `upsertSurface` (:296-307): if id already present, just activate; else append + activate + isOpen=true.
  Every opener (diff, file link, PR, side chat, terminal) goes through this. So "open X" is idempotent.
- Actions (:119-190): open/toggle(kind), activateSurface, revealSurface (automatic, does not count as a
  user action), closeSurface, closeOtherSurfaces, closeSurfacesToRight, moveSurface (drag reorder),
  renameDevice, setPullRequestDetailTab, navigateFolder, split/activate/closeTerminal, setMaximized.
  `openProactive` is refused if the user acted after the revision was read (userActionRevision) —
  agents can't yank the panel from under you.
- Persistence (:1160-1180): zustand `persist`, localStorage key `t3code:right-panel-state:v2`,
  `version: 14`, `migrate: migratePersistedRightPanelState` (validates every surface, drops bad ones,
  restores active id). Closed surfaces are remembered in `closedViewStore` (:1185-1205) for
  "reopen closed tab" (terminals and live browser tabs excluded).
- Docking: the right panel docks RIGHT only. `ChatRightPanel.tsx`: inline beside chat, or a sheet
  (`RightPanelSheet`) under `(max-width: 980px)` (rightPanelLayout.ts:1). Width is user-resizable,
  persisted per `widthStorageKey`. `maximized` makes it take the content area.
  "Bottom" is a separate *terminal drawer* (ChatTerminalDrawers.tsx), toggled by the PanelBottomIcon;
  terminals can ALSO be right-panel tabs. So T3 has: right panel (tabs) + bottom terminal drawer +
  thread-details panel (SquareMenuIcon). Three toggles, but ONE tabbed host for all side content.
- Dispatch: `RightPanelSurfaceContent.tsx:85-237` is a single `switch (surface.kind)` that renders the
  body; heavy ones are `lazy()` + `Suspense fallback={null}`. Side chat deliberately NOT lazy
  (tiptap teardown crash, :173).

### 1.2 Tab strip (components/RightPanelTabs.tsx)
- Bar (:1335-1345): `flex h-[var(--workspace-topbar-height)] items-center gap-1 pl-2 pr-3` (pr-28 inline
  without layout controls, to clear window controls). It IS the titlebar row when inline in Electron
  (`drag-region`, tabs `no-drag`).
- Tab list: ScrollArea `hideScrollbars scrollFade`, inner `flex w-max min-w-full items-center gap-1`.
  dnd-kit sortable, horizontal-axis only, activation distance 6px (:826-850).
  Wheel scrolls horizontally; on overflow, two `icon-xs ghost` chevrons (ChevronLeft/Right) appear,
  scroll by max(120, 75% width), smooth unless reduced motion (:1468-1512).
- Tab (:1007-1020): `group/tab flex h-6 max-w-36 shrink-0 items-center gap-0.5 rounded-md pr-2 pl-1.5 text-xs`
  -> 24px tall, max 144px, radius 6, 12px text.
  active: `bg-accent text-foreground`; inactive: `text-muted-foreground hover:bg-accent/60 hover:text-foreground`.
  No border, no underline, no separators.
- Leading icon doubles as the close button (components/ui/panel-tab-close-button.tsx:21-28):
  a `size-4 rounded-sm hover:bg-muted` button holding the 12px kind icon; on tab hover
  (`group-hover/tab`) the icon is hidden and an `X size-3` shows. So no trailing ×, no width jump.
  Pending dot: `absolute -right-0.5 -bottom-0.5 size-1.5 rounded-full bg-current` over the icon.
  Browser tabs add a mute/unmute 16px button; device tabs rename on double-click
  (`w-24 rounded-sm ring-1 ring-ring` input, Enter commits, Escape reverts).
- Icons per kind (SurfaceIcon :642-697): favicon (browser), FileDiff, Files, TerminalSquare,
  PR glyph tinted by state, Apple/Android/Smartphone, MessagesSquare (side chat, with running dot).
- Middle-click closes (onAuxClick); right-click native menu: Rename / Copy path / ... / Close,
  Close others, Close to the right, Close all (disabled when not applicable) (:1183-1218).
- Title = surfaceTitle(): page title, terminal label, file basename, `#123`, side chat thread title.

### 1.3 "+" add-surface menu (:1393-1462)
- Trigger: `Button size="icon-xs" variant="ghost-muted"` (24px square, muted ink, accent on hover),
  `Plus size-3.5`. Sits right after the last tab, inside the scroll area.
- Shortcut `rightPanel.new` (keybinding) focuses the trigger and opens it.
- MenuPopup `align="start" side="bottom" sideOffset={6}`. Rows: `<Icon/> Label <MenuShortcut>X</MenuShortcut>`.
  Order and keys: Browser B, Terminal T, Files F, Diff D, Pull request P, Linked pull requests L,
  Device M, Side chat S (side chat only where offered).
- While open, a bare letter picks the row (`onKeyDownCapture` -> surfaceShortcutActionForKey, :263-272,
  only *available* actions match).
- Unavailable rows: `disabled` (opacity-64 via MenuItem) but `data-disabled:pointer-events-auto` so a
  tooltip (side top) explains why (`DisabledReasonTooltip`, :284-312).
- Browser with >1 profile becomes a submenu (profiles) — a mouse click still opens default profile.
- Menu shell (components/ui/menu.tsx:57-69): `dropdown-glass rounded-lg` + shadow
  `0 18px 44px -18px rgb(0 0 0/80%)` (dark), padding `p-1`, min-w 10rem. `dropdown-glass`
  (index.css:454): popover at ~80% opacity, `backdrop-filter: blur(12px) saturate(1.14)`,
  1px border foreground@10%.
  Row (menu.tsx:131): `min-h-7 (28px) rounded-sm px-2 py-1 gap-2 text-sm`, icons 16px `opacity-80
  text-muted-foreground`, highlight `bg-accent text-accent-foreground`, disabled `opacity-64`.
  MenuShortcut (:300): `ms-auto text-xs font-medium tracking-widest text-secondary-label` (NOT a key cap).
  Separator `mx-2 my-1 h-px bg-border`; group label `px-2 py-1.5 text-xs font-medium text-muted-foreground`.

### 1.4 Empty state "Open a surface" (:315-567)
- Shown when `activeSurfaceId === null`. Container focusable (`tabIndex=0`, data-workspace-focus-default),
  centred, `max-w-xs py-6`; heading `mb-3 text-center text-sm font-medium text-foreground`
  "Open a surface"; list `flex flex-col gap-0.5`.
- Row: `flex h-8 w-full items-center gap-2.5 rounded-(--control-radius) px-2.5 text-sm` (32px, radius 8),
  icon 16px, label flex-1, then `<Kbd>` key cap. Hover/highlight `bg-accent/60`.
- Disabled rows: same row at `opacity-50`, `aria-disabled`, still focusable, tooltip gives reason.
- Keys: bare letter fires (window capture listener, skipped while typing or another layer is open);
  Arrow Up/Down AND Left/Right move a highlight, wrapping; Enter fires.
- Kbd (components/ui/kbd.tsx): `h-5 min-w-5 rounded px-1 text-xs font-medium font-sans
  text-muted-foreground bg-foreground/[0.07]` — 20px soft square, no border (outline variant has
  `border border-b-2 border-foreground/15`).

### 1.5 Layout controls (components/chat/PanelLayoutControls.tsx)
- `flex h-full items-center gap-1`. Three `Toggle variant="ghost" size="sm"` (28px square at sm,
  radius-lg 8, pressed `bg-input/64`), icons 16px at 80% opacity:
  SquareMenuIcon "Toggle thread details", PanelBottomIcon "Toggle terminal drawer",
  PanelRightIcon "Toggle right panel". Tooltips side bottom with shortcut in parens.
- Maximize (:135-162): same Toggle, `MorphIcon` Maximize2 <-> Minimize2, "Maximize panel" /
  "Restore panel size". Only rendered while panel open; fades in with panel animation (:176-213).
- These live in the window titlebar's top-right (fixed), so the panel's tab strip shares the row.

### 1.6 Side chat surface (components/chat/SideChatPane.tsx:130-180)
- Head: `flex h-9 items-center gap-1 border-b border-border pr-1 pl-3` (36px).
  Title `flex-1 truncate text-xs font-medium`; `Button ghost xs` "Bring back" (aria-disabled with
  reason tooltip: "Available when the main thread is idle." / "Available after a finished reply.");
  `icon-xs ghost` Trash2 (confirm dialog if setting on), `icon-xs ghost` X close.
- Body: full `ChatView` of the side thread, embedded. Timeline header divider
  (MessagesTimeline.tsx:1078-1092): `TimelineSystemDivider label="Context from" detail=<parent title>`
  icon MessagesSquare (subagents use "Subagent of" + Bot icon), action "Open parent thread".
- Bring back = server `mergeBack` of the latest run into the parent; toast "Brought back".

### 1.7 Pull request surface (components/pullRequest/PullRequestDetailPanel.tsx)
- Header grid `grid-cols-[minmax(0,1fr)_auto] gap-x-2 border-b border-border/60` (:1288).
  Left: crumbs (`pl-4 h-7`, repo / #n, back arrow, external link). Right actions `mr-4 h-7 gap-1`:
  stack menu, primary action (Merge/... with state presentation — "Merged" in
  `text-violet-600 dark:text-violet-300/90`, pullRequestIcons.tsx:48-52), checkout, overflow
  `PullRequestActionsMenu` (ask/explain/fix findings/write guide/merge method/delete branch/copy...),
  optional collapse (PanelRightIcon size-3.5).
- Tabs (TABS :200-205): Summary, Guide, Timeline, Code (unavailable ones filtered). Rendered by
  `PullRequestDetailTabBar` (PullRequestDetailLayout.tsx:81-108): nav `px-4 py-2 gap-2 border-t
  border-border/60`, `ToggleGroup variant="segmented"`:
  group `gap-0.5 rounded-lg bg-input/40 p-0.5` (toggle-group.tsx:32);
  item `h-6 rounded-md px-2.5 text-xs text-muted-foreground hover:bg-input/32`, pressed
  `bg-input/72 text-foreground shadow-xs/10` (toggle.tsx:19,31). Trailing slot: checks status line,
  comment/commit counts, sort toggle. Code tab is lazy and prefetched on hover/focus.
  Selected detail tab persists on the surface (`detailTab`).

### 1.8 Diff surface toolbar (components/DiffPanel.tsx:817-1270, DiffPanelShell.tsx:10-18)
- Embedded header row: `flex items-center justify-between gap-2 px-2 h-7` (no border inline).
- Left: scope dropdown `Button size="xs" variant="secondary"` (Uncommitted/branch/turn/commit) with
  ChevronDown 3.5; comparison label "Comparing <head> against <base>" + change-base dropdown.
- Right: Refresh, Expand/Collapse all, ToggleGroup Stacked (Rows3) / Split (Columns2), toggles:
  Wrap (TextWrap), whitespace (Pilcrow), Hide test files (FlaskConicalOff), file tree (FolderTree).
  All 14px icons, tooltips side top.

---------------------------------------------------------------------------------------------------
## 2. T3 header breadcrumb (components/chat/ChatHeader.tsx:241-399, WorkspaceBreadcrumb.tsx)
- `<nav><ol class="flex items-center gap-2 sm:gap-3 text-sm">`. Items `font-medium`; non-current
  `shrink-0 text-muted-foreground`, current `text-foreground`. Separators are a literal `/` in
  `text-icon-muted`. Text uses `text-box: trim-both cap alphabetic` + py-1.5 for optical centring.
- Project: button with `ProjectFavicon size-3.5` (letter badge like "RE" when no favicon, coloured by
  project icon colour) + name `max-w-40`; click = "New thread in <project>" (tooltip).
- Parent thread (subagent/side-chat child): muted button -> opens parent. When the row overflows a
  ResizeObserver collapses the parent title to `...` (:118-150), full title in tooltip.
- Current thread: `<h2>` title button; ChevronDown 3.5 appears only on hover/focus
  (`opacity-0 group-hover/thread-title:opacity-100`); click opens thread menu, double-click renames
  inline (input `ring-1 ring-inset ring-ring/50`, Enter commits, Escape cancels, empty rejected).
  Right-click anywhere on header = same menu. "From <env>" info Badge for linked-env threads.
- Container `pr-10` with panel open, `pr-24` otherwise (room for titlebar controls).

---------------------------------------------------------------------------------------------------
## 3. T3 composer

### 3.1 Surface (components/chat/ComposerSurface.tsx)
- Shell: `max-w-(--chat-content-max-width)`, glass via `before:` `rounded-2xl (16px)`
  bg card/surface-raised at --glass-opacity, `backdrop-blur(12px) saturate(1.14)`.
- Outline: `after:border` rgb(0 0 0/8%) light, white 5% dark, plus dark inset highlight white 3%.
- Shadow: `--shadow-composer: 0 1px 2px rgb(0 0 0/5%)` light, none in dark.
- ContextStrip under the card: `flex gap-2 px-2 pt-2 text-muted-foreground` (env / worktree / branch).
- Placeholder (composerPlaceholder.ts): `Ask anything. @ files, $ skills, / commands`.
- Command menu (ComposerCommandMenu.tsx:129-175): a drawer attached to the top of the composer
  (ComposerBanner.Surface), list `max-h-72`, items `rounded-lg`, row: icon 16 secondary-label,
  name `text-xs font-medium max-w-[45%]` (skills prefixed `/skill:` in secondary label), description
  `text-xs text-secondary-label`; empty/loading text `px-5 pt-3.5 pb-7 text-xs` ("Searching workspace
  skills..."). Active item scrolled into view.

### 3.2 Inline chips: skills, files, models (the purple cube)
- Editor is Tiptap. Skill = inline ATOM node `composer-skill` (ComposerPromptEditorTiptap.tsx:309-331)
  attrs {skillName, skillLabel, skillDescription}; model = `composer-model`; file = `composer-mention`.
  Atom => caret jumps over it as one unit, Backspace deletes the whole chip, it can be node-selected:
  selected chip draws `after:bg-[Highlight] opacity-30 rounded-sm` (:244-245). Undo restores chips
  (composerContextUndo.ts); stale chips re-labelled when skills list loads (:1622-1632).
- Stored as plain text: serialiser writes `$<name>` for skills (composer-rich-text-doc.ts:1020-1022),
  `@path` for files, `model:<instance>/<model>` for models; parsed back on load. Sent prompts render
  the same chips via SkillInlineText.tsx (`matchInlineSkills` over message text).
- Look (ContextChip.tsx:8-50): NO pill background. `inline-flex gap-[0.2em] font-medium rounded-sm`,
  text colour = kind accent, svg `size-[1em]` self-centred. Label underline appears only on hover
  (decoration transparent -> current). Accents (lib/contextChipAccents.ts), all L=0.62:
  skill `oklch(0.62 0.16 322)` (magenta-purple), file/image `oklch(0.62 0.136 237)` (blue),
  mention 215, terminal 163 (green), model `oklch(0.62 0.13 105)` (olive), thread 190, PR open 163,
  merged 292, closed 16.
- Skill icon = lucide "box"/package cube, stroke 1.85 (composerInlineChip.ts:1). File chips use the
  file-type icon (PierreEntryIcon). Click opens a popover: name, description, "View instructions".

### 3.3 Footer controls (ComposerControl.tsx, ComposerFooterControls.tsx)
- Control: `h-7 (sm:h-6 for xs) rounded-(--control-radius) px-2 gap-1.5 text-sm font-medium
  text-foreground/80`, icons muted, `hover:bg-accent`, active scale 0.97.
- Model trigger (ProviderModelPicker.tsx:356-430): provider icon 16 + model name (foreground/80) +
  suffix (ComposerModelEffort.tsx:146-175) in `text-muted-foreground` that turns foreground on
  hover/open: effort word ("Extra High") or EffortGlyph bars, FastBolt, Ultracode workflow icon,
  context-window label. => "Opus 5.5" brighter, "Extra High" dimmer.
- Permissions chip (FooterControls :24-104): icon + label, menu with segmented radio row
  (`flex rounded-lg bg-input/40 p-0.5`, segment `h-6 text-muted-foreground data-checked:bg-input/72`),
  hovered mode's description previewed beneath (`min-h-8 text-xs`).
- Workspace menu (BranchToolbarEnvModeSelector.tsx:76-170, BranchToolbar.logic.ts:93,153):
  `h-7 sm:h-6 px-1.75 text-sm sm:text-xs text-muted-foreground/70`, Folder/FolderGit/FolderGit2 3px icons;
  options "Current checkout" (or current worktree label), "New worktree", "Previous worktree (<branch>)";
  popup widens to `min(21rem, 100vw-2rem)` when Previous is offered.

### 3.4 Send / stop (ComposerPrimaryActions.tsx)
- Send (:254-297): `h-9 w-9 sm:h-8 sm:w-8 rounded-full shadow-xs hover:scale-105`, bg
  `message-action` (= primary blue) + `inset-shadow 0 1px rgb(255 255 255/16%)`; disabled
  `bg-foreground/12 text-muted-foreground`. Glyph: custom 14px up-arrow, stroke 1.8, round caps;
  ListPlus/CornerUpRight while running (queue/steer), Check when editing queued, Play to resume,
  Spinner while busy.
  The "gradient orb" is `StageBackdropButtonArt` (SidebarStageBackdrop.tsx:67) — only on nightly/dev
  builds (`stageBackdropVariant`): night-sky / blueprint SVG art behind a transparent button,
  `hover:brightness-110`. Release builds are solid primary.
- Stop (:125-150): `size-8 sm:size-7 rounded-full bg-destructive/90 text-white shadow-xs
  shadow-destructive/24`, hover full destructive, glyph 8x8 rounded square (rx 1.5). Tooltip "Interrupt".
  Shown instead of send while running with empty input; beside send when there's text.

### 3.5 Model picker (ProviderModelPicker.tsx, ModelPickerContent.tsx, ModelPickerSidebar.tsx, ComposerModelEffort.tsx)
- Popover, docked: opens `side="top"` above the composer, padding none. Fixed height `h-86.5` (346px) so
  switching models never resizes. Layout: [provider rail 44px][model list max-w-90 (360px)][effort column 48 (192px)]
  + footer.
- Provider rail (Sidebar :72-180): `w-11 bg-muted/30 p-1 gap-1`; square buttons `aspect-square rounded-md
  hover:bg-foreground/10`, provider icon 24/20px; Favorites star first, then a hairline; selected indicator
  = `absolute right-1 h-5 w-0.75 rounded-full bg-primary` sliding (`transition-[top] 200ms ease-out`).
  Unavailable providers 50% opacity with reason tooltip.
- List: `bg-muted/40 border-l border-border/70`; search "Search models..." on top (menu search input style:
  border-b, 26px input, search icon muted/55). Row (ModelListRow.tsx): provider icon 16, name `text-sm`,
  Check when selected, sub-provider `text-xs muted/70`, "New" badge (3xs bold uppercase, update colour),
  favourite star (appears on hover, filled `text-warning` when favourited), then `<Kbd>` jump label.
  ⌘1..⌘N jump to models (`modelPickerJumpIndexFromCommand`, ModelPickerContent.tsx:790). Searching hides the rail
  and ranks across providers.
- Effort column (ComposerModelEffort.tsx): `w-48 border-l border-border/70 bg-muted/40`; head
  `h-8.25 border-b px-3 pt-2.5 text-sm` "Reasoning" + model label right in xs muted; rows
  `h-8 rounded-sm px-2 text-sm hover:bg-foreground/[0.09]` radio, Check on selected, "Default" xs muted/70;
  column labels `px-2 pt-1.5 pb-1 text-xs font-medium text-muted-foreground`. Up/Down move, Left returns to search.
- Footer (`ComposerPickerFooter`, :~665): `flex gap-2 border-t border-border/70 bg-muted/40 px-3 py-2`:
  Speed segmented (Standard | Fast, bolt), Ultracode segmented toggle (workflow icon), and permissions as
  `ms-auto h-7 rounded-md px-2 text-xs` menu ("Full access" etc, ChevronUp). Segment style FOOTER_SEGMENT (:525).

### 3.6 Empty-thread hero (DraftHeroHeadline.tsx, DraftHeroProjectHeading.tsx:48)
- `h1 text-center font-normal text-2xl sm:text-3xl tracking-tight`: "What should we build in <project>?"
  with project name `font-medium` and itself a project picker (dotted underline, muted/60). Inline on lg if it
  fits, else stacks; hidden below 28rem height. Scratch: "What should we work on?".

---------------------------------------------------------------------------------------------------
## 4. Ferrite today

### 4.1 Side-surface concepts (crates/ferrite-cockpit/src/cockpit/)
Hosts / bodies that can occupy the right side or beside the Pane:
1. Panel rail: "one scroll of sections with no tabs" — Environment, Context, Connectors, Changes, Background
   (panel.rs:1-35, panel/sections.rs). PANEL_W 320.
2. Panel head tabs: `Overview` + the Preview tab, only while a preview is open (panel.rs `panel_head` ~:349,
   `overview_tab` ~:390), PANEL_TAB_H 28, pad 10, R_CONTROL 6, FS_LABEL 13/500; plus an `Open browser` head door.
3. Preview (file at line / markdown / command output / page / live browser, ADR 0036) widening the panel
   (42% clamp 420–820) with its own expand + × (panel/preview.rs).
4. Lightbox for images (separate overlay).
5. Side chat column: own tab strip, `+ New side chat`, expand, close; SIDE_CHAT_W 420 (side_chat.rs:617+).
6. Agent drill-in column: `‹ task`, expand, close (agent_pane.rs:193).
7. Background tasks pane: head + close (panel/background.rs:91).
8. Dock / Files: editor tab strip (preview tabs italic), Go to file, Collapse all, Hide tree, ⋯, expand, close
   (tools/files.rs). FILES_SHARE 0.6, min 420.
9. Dock / Changes: scope dropdown, ⋯, expand, close; stacked under Files in the same column (tools/changes.rs).
10. Dock / Terminal: own column TERMINAL_W 420, own tab strip `Terminal N`, +, ⋯, expand, close (tools/terminal.rs).
11. Review slot: inside the board beside the Pane, sliding (beside.rs, ADR 0021).
12. Another Thread beside (second Pane).
Entry points: title-bar doors (view_head.rs:194-277) — Open a thread beside, Show session rail (⌥⌘B),
Terminal, Changes (⇧⌘D), Files (⇧⌘F), ⋯ — six 28px icon buttons, pressed = `paint::SELECTED`.
Counting: ~12 surface concepts, 4 separate tab-strip implementations (panel head, side chat, files editor,
terminal), 6 separate expand doors, ~9 separate close buttons, 5 width rules (PANEL_W, preview_w, SIDE_CHAT_W,
TERMINAL_W, FILES_SHARE + DOCK_THREAD_MIN_W) and a precedence ladder in `panel_element`
(agent drill > side chat > background > preview > sections, dock alongside) — state spread over
`Panel`, `Tools{docks,files,changes,terminals}`, `SideChats`, `PaneView::preview`, browser map, ReviewSlot.
Tool enum (tools.rs:28-36): Rail, Terminal, Changes, Files; Dock {files, changes, terminal} bools.
Nothing is persisted across restarts except `Settings::panel_shown`.
T3: 1 surface union, 1 tab strip, 1 "+" menu, 1 empty state, 1 close affordance, 1 maximize, 1 width.

### 4.2 Header (view_head.rs:279-330)
Solo: title (FS_TITLE 14, W_LABEL) + ▾ menu, branch in INK_SUBTLE while panel hidden. No project, no parent
crumb (project is in nav). Double-click renames (same as T3). No `/` breadcrumb, no subagent parent link.

### 4.3 Composer (crates/ferrite-pane/src/pane/composer.rs, composer.rs, ferrite-ui/src/theme.rs)
- Placeholder: "Describe a task or ask a question" (empty), "↵ to steer this turn" (running), idle text
  (pane/composer.rs:900-926). No hint of @ / $ / / grammar.
- Model chip already matches the screenshot: model INK_STRONG W_LABEL + effort INK_SUBTLE W_BODY,
  FOOTER_WORD_GAP (pane/composer.rs:16-45); shimmer while session loads. Mode chip amber for Manual.
- Footer chips: FOOTER_CHIP_H 24, R_CONTROL 6, no ground at rest, RAISED on hover. Attach `+` 30px.
- Send: COMPOSER_SEND 28 round; lit = PRIMARY (= INK_STRONG #efefef, ON_PRIMARY #161616 arrow) — white
  orb, not blue/gradient; empty = paint::SEND + INK_SUBTLE arrow; stop = paint::SEND + INK_STRONG stop
  glyph (not red). Glyph morph arrow<->stop (FADE_QUICK) (pane/composer.rs:109-226).
- Inline tokens (composer.rs:150-165, 616-631, 1098-1200, 1236-1340): the line is a plain string; chips are
  paint over byte ranges. `@path` picked mentions = LINK_INK #7aa8ee on MENTION_CHIP ground (blue 16%,
  R_CONTROL, pad 3, inset 1). Skills/commands `/name` = SKILL_INK #5ea1ea, no ground, NO icon.
  Backspace removes one character (line.rs:184) — the token then stops matching and loses its colour;
  caret walks through the token char by char. Skills use `/` (Claude's grammar), not `$`.
- Model/effort pickers are separate floats (cockpit.rs Kind::Provider, Kind::Effort): FLOAT_ROW_H 28,
  R_FLOAT 12, opaque paint::FLOAT #303030 (no glass), MENU_W 160. No provider rail, no search-across-providers,
  no ⌘N jump labels, no docked effort column, no favourites.

---------------------------------------------------------------------------------------------------
## 5. Consolidation sketch: one Surface host for Ferrite

```rust
// crates/ferrite-cockpit/src/cockpit/surfaces.rs
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceId {               // the id is the dedupe key (T3's kind-prefixed ids)
    Overview,                      // today's rail sections (Environment/Context/Connectors/Changes/Background)
    Changes,                       // dock Changes (singleton)
    Files,                         // tree + editor; editor's own file tabs stay inside (like T3's terminal splits)
    File(PathBuf),                 // today's Preview of a file/markdown/output
    Browser(TabId),                // ADR 0036 tab
    Terminal(TermId),              // splits inside
    SideChat(ThreadId),            // one tab per fork, replaces side chat's own strip
    Agent(SubagentId),             // drill-in
    Background,                    // background tasks pane
    PullRequest(PrRef),            // with remembered sub-tab (Summary/Timeline/Code)
}
pub struct ThreadSurfaces { open: bool, active: Option<SurfaceId>, tabs: Vec<SurfaceId>,
                            maximized: bool, width: f32 }
// HashMap<ThreadId, ThreadSurfaces>, persisted in the Store (versioned + migrate), like T3's v14.
```
- One API: `upsert(thread, id)` (activate if present), `activate`, `close`, `close_others`, `close_right`,
  `move_to`, `reveal` (agent-initiated, refused if operator acted since — T3's userActionRevision).
  Every current opener (open_preview, open_in_files, press_tool, side chat start, agent drill, open_browser,
  Show changes) calls `upsert`. Delete the precedence ladder in `panel_element`: the body is
  `match active { … }` (T3's RightPanelSurfaceContent switch); each module keeps only its body + an optional
  sub-toolbar row (Diff scope dropdown, PR segmented tabs, terminal splits).
- One head row (the titlebar row over the column, WIN_CHROME_H): tab strip (24px tabs, max ~144px,
  R_CONTROL 6, FS 12, active = paint::SELECTED + INK_STRONG, inactive INK_SUBTLE + hover RAISED; leading kind icon
  that turns into × on tab hover; middle-click close; drag reorder; horizontal scroll + chevrons on overflow),
  then `+`, then maximize (expand) — ONE expand and ONE close model for every surface.
- `+` menu and empty state share one action table: Browser B, Terminal T, Files F, Changes D, Pull request P,
  Side chat S, Background G, Overview O — disabled rows dimmed with reason tooltip, bare letter picks,
  arrows move, ↵ opens. Use a key-cap Kbd (20px, INK_SUBTLE on a 7% ground) in the empty state, a plain
  trailing letter in the menu.
- Title tools collapse from six doors to T3's three-ish: Toggle panel (⌥⌘B), optionally a bottom terminal
  drawer toggle, and maximize while open; Terminal/Changes/Files doors become `+` entries / ⇧⌘D ⇧⌘F shortcuts
  that `upsert`.
- Width: one per-thread (or global) resizable width with a per-kind default floor (Overview 320,
  File/Browser 42% clamp 420–820, Files 60%); maximized = all but DOCK_THREAD_MIN_W. Drop the side-by-side
  dock (Files | Terminal columns); if wanted later, add a split inside the host rather than separate columns.
- Review slot (in-board, ADR 0021) and Thread-beside stay separate — they're board layout, not side surfaces;
  but the Changes surface can offer "Open review beside".
- Migration order: (1) introduce SurfaceId/ThreadSurfaces + head strip rendering existing bodies;
  (2) route openers through upsert; (3) delete side chat / terminal / files-dock tab strips and their
  expand/close doors; (4) persist; (5) "+" menu + empty state; (6) trim title tools. Needs an ADR
  (supersedes panel.rs "no tabs" doc + CONTEXT.md **Panel**, which already says "one tab each").

### Composer follow-ups worth copying
- Skills as atomic tokens: keep plain-text storage but make the token range atomic in `Line`
  (backspace/arrow treat a chip range as one unit), add the cube icon and a distinct skill accent
  (T3 magenta oklch(0.62 0.16 322) ≈ #b65fc0) vs file blue; underline-on-hover; click -> description popover.
- Placeholder that teaches the grammar ("Ask anything. @ files, / skills").
- Model picker: provider rail + search + ⌘1..N + docked effort column + footer (speed/permissions), fixed height.
- Stop button in a danger tint; send in an accent (Ferrite's PRIMARY is white).
