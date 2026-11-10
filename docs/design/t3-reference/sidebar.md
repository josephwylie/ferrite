# T3 Code sidebar: how it's built, and Ferrite's gaps

Source: pingdotgg/t3code at `bcee0f5d`, `apps/web/src` (paths below are relative to it unless absolute).
Units: Tailwind v4, 1 spacing unit = 4px, `text-xs` = 12px/16px, `text-sm` = 14px/20px, `text-2xs` = 11px/16px, `text-3xs` = 10px/14px.
`--radius` = 8px, so `rounded-md` = 6px, `rounded-sm` = 4px, `rounded` = 4px, `--control-radius` = 8px.
Default Tailwind transition = 150ms `cubic-bezier(0.4,0,0.2,1)`.

---------------------------------------------------------------------------
## 0. Tokens: dark mode as the sidebar sees it

The sidebar root has `data-app-sidebar` (AppSidebarLayout.tsx:466). That scope overrides the global palette (index.css:1234-1278):

| token | dark value | notes |
|---|---|---|
| `--sidebar` (column bg) | `--card` = `#000` | plus `surface-grain` (fractal-noise SVG, opacity .035, 256px tile, index.css:1098,1769) on `sidebar-inner` (ui/sidebar.tsx:386) |
| `--foreground` | `#f1f3f7` | titles |
| `--muted-foreground` / `--sidebar-muted-foreground` | `#a3a3a3` | section headers, search text, Waiting status |
| `--secondary-label` | `#8f8f8f` | "a step below read titles"; project name, branch, time, PR number |
| `--sidebar-row-hover` | `color-mix(srgb, foreground 8%, transparent)` ≈ white 8% | hover fill |
| `--sidebar-row-active` | foreground 11% | current route's row |
| `--sidebar-row-selected` | foreground 7% | multi-selected rows |
| `--border` / `--sidebar-border` | `rgb(255 255 255 / 8%)` | |
| `--accent` | `#191a1d` | PR popover row hover |
| `--info-foreground` | blue-400 (`oklch(.707 .165 254.6)` ≈ #51a2ff) | Working |
| `--success-foreground` | emerald-400 | Done |
| `--warning-foreground` | amber-400 | Approval, Limited, Woke |
| `--error-foreground` | red-400 | Failed |
| Input | `text-indigo-300` | |
| Request | `--guest-foreground` fuchsia-300 | |
| `--sidebar-icon-color` | `color-mix(muted-fg 85%, sidebar)` | footer/header glyphs (index.css:106) |

Geometry tokens (index.css:104-111): `--sidebar-content-inset` 8px, `--sidebar-control-gap` 8px, `--sidebar-row-content-inset` 10px, `--workspace-topbar-height` 52px.

Sidebar width (threadSidebarWidth.ts:2-4): default 256px, min 208px (or the brand width), max = viewport − 640px. Resizable; double-clicking the rail resets it (AppSidebarLayout.tsx:484). Collapsing is offcanvas: `transition-[left,right,width]` over `--panel-animation-duration`, ease-out (ui/sidebar.tsx:363,374).

---------------------------------------------------------------------------
## 1. Column layout, top to bottom

`Sidebar.tsx:2993-3683`:
1. **Titlebar** `SidebarChromeHeader` (sidebar/SidebarChrome.tsx:43-94): height 52px, `pr-3`, a drag region. Brand: the "T3" wordmark plus "Code" in `text-sm font-medium tracking-tight`; "Code" is `text-muted-foreground`, or `text-white/70` on the art. Electron adds back/forward `icon-sm` buttons with 14px arrows.
   - **Starry gradient** = `SidebarStageBackdrop` (SidebarStageBackdrop.tsx:52-215). It is drawn only for **nightly/dev** builds (`resolveSidebarStageBackdropVariant`). It is an absolute `inset-x-0 top-0 h-20` (80px) SVG, `preserveAspectRatio xMinYMin slice`, with these layers:
     - Sky: a linear gradient (`--stage-night-bottom` oklch(.20 .056 261), mid oklch(.227 .086 278), top oklch(.284 .117 297)) with reflect spread.
     - Glow: a radial gradient at (216,18), rotated 137°, scale 120×84, highlight oklch(.554 .177 272) at 40% opacity, falling to the secondary at 16%.
     - Stars: a pattern tiled every 288×96 with 17 dots (r 0.4–0.6, opacity .45–.85, fill `--stage-night-line` oklch(.939 .029 273)) and 3 "+" sparkles (stroke .6, opacity .7).
     - Two cloud paths blurred `stdDeviation 4`, filled with a linear gradient (highlight → secondary → tertiary at .5/.62/.5).
     - Mask `linear-gradient(to bottom, black 0 55%, transparent 92%)` plus an `::after` fade into `--sidebar` (index.css:528-549). The art bleeds under the search row.
   - Header controls on the art switch to the `media-navigation` button variant.
2. **Fixed header** (not scrolled) `SidebarThreadHeader` (sidebar/SidebarThreadHeader.tsx:59-131), in a `SidebarGroup` with `p-2` (8px) and the first group's `pt-0`:
   - The row is `flex items-center gap-1`.
   - **Search field:** `h-8 flex-1 gap-2 rounded-md px-2 py-1.5 text-sm font-medium text-sidebar-muted-foreground`.
     - Hover and focus-within: `bg-sidebar-row-hover`, `text-sidebar-foreground`; focus also adds `ring-1 ring-inset ring-ring/70`.
     - Contents: a 16px `SearchIcon` in `--sidebar-icon-color` and an unstyled input with placeholder "Search".
     - While searching, a clear button `icon-micro ghost-muted` with a 12px X.
   - **Icon buttons:** `SidebarHeaderIconButton` = `SidebarMenuButton size=icon` forced to `size-7` (28px), glyph 16px, tooltip side=top. A hidden 48px touch target only shows on coarse pointers.
     - **Filter** (project scope combobox). The trigger shows the scoped project's favicon (16px), or two overlapped 14px favicons (`-ml-1 ring-2 ring-sidebar`) when two are scoped.
     - **FolderPlus** "Add project".
     - **SquarePen** "New thread (⌘N)". Its tooltip adds "New thread in current project: Shift+click".
3. **Scroll area** `SidebarContent` (ui/sidebar.tsx:652-684):
   - Scrollbars are hidden; a scroll-fade mask of **12px** (`--fade-size: .75rem`) applies at whichever edge has overflow.
   - `overflow-anchor:none`, and the content has `min-h-full`.
   - Inside it, a `SidebarGroup flex-1` with `p-2` holds the `<ul class="relative flex flex-col gap-px flex-1">` (Sidebar.tsx:3328-3340). The list gap is **1px**.
4. **Footer** `SidebarChromeFooter` (SidebarChrome.tsx:221-302):
   - `SidebarFooter`: `px-2 pt-1 pb-2 flex-col gap-2`.
   - It holds `SidebarMenu flex-row items-center` with three `SidebarMenuButton size=icon` (32px square, rounded 8px, glyph 16px in `--sidebar-icon-color`). On hover they take `bg-sidebar-row-hover` with the glyph in foreground; on active, `bg-sidebar-row-active`.
   - In order: **Settings**, **Pull requests** (`GitPullRequestArrow`), **Usage** (`ChartNoAxesColumn`). Each has a tooltip on top.
   - Then `SidebarThreadUndoNotice`: it fills the free middle with `text-2xs leading-6`, muted, e.g. "Settled, Undo ⌘Z". It fades in over **200ms ease-out** using `@starting-style` (`starting:opacity-0`), fades out the same way, and unmounts on `transitionend` (SidebarThreadUndoNotice.tsx:45-58).
   - Then `SidebarUpdatePill` `ml-auto shrink-0`. This is the **up-arrow button**: the desktop update status icon, with a release-notes popover (SidebarUpdatePill.tsx:510).

### Bottom anchoring
The list is `flex-1 flex-col`. The **first present shelf header gets `mt-auto`**: Working gets it if present, else Snoozed if Working is empty, else Settled (Sidebar.tsx:3553, 3568, 3584). So pinned and active cards stack from the **top**, and the Working/Snoozed/Settled shelves sit pinned to the **bottom**, right above the footer. When the list overflows it just scrolls.

---------------------------------------------------------------------------
## 2. Sections and ordering

`partitionSidebarThreads` (Sidebar.tsx:618-706) sorts each thread into a section by `resolveSidebarThreadSection` (Sidebar.logic.ts:176): snoozed > settled > pinned > active. With the optional `sidebarWorkingShelfEnabled` setting, active threads that are working go to `working` instead (Sidebar.tsx:442, 630).

| section | variant | sort |
|---|---|---|
| Pinned | card | pin order key |
| Active | **card** | `sortThreadsForSidebar`, or by "inbox return" (max of createdAt/unsettledAt/observed return) when the working shelf is on |
| Working shelf | **card** | `sortWorkingThreadsBySend`: newest send first |
| Snoozed shelf | slim | `snoozedUntil` ascending |
| Settled shelf | slim | `sortSettledThreads`: settled timestamp descending, then id |

- **Shelves start collapsed:** `useLocalStorage(..., false)` (Sidebar.tsx:451-465), persisted per shelf.
- **A collapsed shelf still shows the route's own thread** (`routeThreadOnly`). The open thread never disappears.
- **Settled pages:** the first 10 show (`SETTLED_TAIL_INITIAL_COUNT`), then a "Show 25 more" row. That row is `h-9 rounded-md px-2.5 gap-2.5 text-sm text-sidebar-muted-foreground`, hover `bg-sidebar-row-hover` with foreground text, and leads with a 16px ChevronDown. "Plus means add, so revealing more uses a chevron" (Sidebar.tsx:3628-3647). When paging fails it reads "Loading" or "Could not load older threads. Retry".
- **Draft block:** unsent new-thread drafts render first (`SidebarDraftBlock`). Each is a card-shaped row with a 12px SquarePen in warning, a `bg-warning/4` surface (hover `/8`), and a hover X to discard.

### Shelf header `SidebarSectionHeader` → `CollapsibleSectionHeader`
(sidebar/SidebarListMarkers.tsx:113-147, ui/collapsible-section-header.tsx:11-48)
- The `li` is `relative mx-0.5 h-8` (2px side inset, 32px tall).
- The button is `flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-xs font-medium`. Its tone is `muted` = `text-sidebar-muted-foreground` (#a3a3a3). During a drag it becomes `emphasized` (foreground/80); as a drop target it becomes `accent` (primary).
- Children in order:
  1. The label ("Working" / "Snoozed" / "Settled"), truncated.
  2. A `h-px min-w-2 flex-1` rule in `bg-sidebar-border/50`, made **`invisible` when a count accessory is present**: "a header with a trailing count drops its rule so it doesn't read as a leader". So the rule only takes up space; the count sits flush right.
  3. The count, `font-normal tabular-nums` (it inherits muted 12px).
  4. A **12px ChevronDown**, `transition-transform` (150ms default ease), rotated `-rotate-90` when collapsed. So collapsed it points right, open it points down.
- Focus: `ring-2 ring-inset ring-ring/70`. No hover background on headers.
- **Expand/collapse is not animated.** Rows just appear or vanish. The list FLIP is **explicitly suppressed** on shelf toggles: "Shelf rows would travel over stationary headers below the bottom-anchored shelf" (Sidebar.tsx:2002-2012). Only the chevron rotates.
- Keyboard: ArrowRight on a collapsed header expands it; ArrowLeft collapses it (lib/sidebarTree.ts:125-139).

---------------------------------------------------------------------------
## 3. The card (variant "card")

sidebar/SidebarThreadRow.tsx:1402-1601.

### Box
- `li`: `list-none py-0.5` (2px above and below), `content-visibility:auto` with intrinsic size 78px (58px when the meta line is empty). The 1px `gap-px` of the `ul` sits between items, so the card pitch is 78 + 4 + 1 = **83px**.
- Row surface (`rowSurfaceClassName`, :957-978): `group/sidebar-row relative w-full cursor-pointer overflow-hidden rounded-md text-left outline-none select-none`, focus-visible `ring-2 ring-inset ring-ring`.
  - Active (route): `bg-sidebar-row-active` (white 11%).
  - Selected or swept: `bg-sidebar-row-selected` (7%).
  - Draft: `bg-warning/4 hover:bg-warning/8`.
  - Receded: `text-sidebar-muted-foreground/75 hover:bg-sidebar-row-hover hover:text-sidebar-foreground`.
  - Default: `bg-transparent hover:bg-sidebar-row-hover`.
  - **There is no transition on the background: hover snaps.** In dark mode the card has **no border and no fill at rest**; the "card" only appears as a fill on hover or when active. Light mode adds `ring-1 ring-inset ring-sidebar-border/70` on active and selected.
  - Being dragged: `bg-sidebar bg-linear-to-b from-sidebar-row-active to-sidebar-row-active opacity-100 shadow-lg`, with the `li` at `relative z-20`.
  - File drag over the card: `ring-1 ring-inset ring-primary/70`.
- Inner box (:1435-1440): `relative z-10 h-[4.875rem]` (**78px fixed**, `h-auto` when the meta line is empty), `px-(--sidebar-row-content-inset)` (10px), `py-(--sidebar-content-inset)` (8px).
- Vertical budget: 8 + row1 20 + 4 + row2 20 + 2 + row3 16 + 8 = 78.

### "Receding" (important feel detail)
`shouldRecedeSidebarThread` (Sidebar.logic.ts:827-849) is true for **working, preparing and waiting** threads, and for read ready/approval threads. It is false for the active or selected thread, for input/request, and for unread or woke threads.
- A receded **working** card gets `opacity-70 transition-opacity hover:opacity-100 focus-within:opacity-100 motion-reduce:transition-none` (:971-973). So running cards sit at 70% opacity and come up to full on hover with a 150ms fade.
- The title of a receded card drops to `font-normal text-foreground/75`, and back to foreground on hover (:1029-1039).
- A non-receded card uses `font-medium text-foreground`. A failed-and-seen card uses `text-foreground/95`.
- **Net effect:** what *needs you* (done, unread, input, failed) reads brightest. Busy agents are deliberately quieter.

### Row 1: project, status (`flex h-5 min-w-0 items-center gap-1.5`, :1441)
- Draft pen (12px warning), only when there is an unsent draft.
- **Project badge** `ProjectFavicon size-4` (16px). Without a favicon it falls back to `ProjectMonogram` (ProjectMonogram.tsx:7-48):
  - A 16×16 SVG, `rounded-[25%]` (4px), background `color-mix(currentColor 14%, transparent)`.
  - Text: `font-mono`, **8.25px, weight 700**, centered at y=10.8, `textLength` 12 (6 for one glyph) with `lengthAdjust=spacingAndGlyphs`, so two letters always fill the tile width exactly.
  - Examples: "relay" → "RY" (one word: first + last glyph), "react-explorer" → "RE" (first glyph of first and last words), "web2" → "W2".
  - Colour: `index = (index*31 + codepoint) % 17` over the lowercased NFKC name, picking from 17 Tailwind hues (gray, red, orange, amber, yellow, lime, green, emerald, teal, cyan, sky, blue, indigo, violet, purple, fuchsia, pink, rose). In dark mode it uses the **-400** shade as text and that colour at 14% as the tile (projectIconColors.ts).
  - A real favicon renders as an `img rounded-[25%] object-contain`.
- Pin indicator (12px, `text-muted-foreground/65`, turns into PinOff on hover). It sits left of the status slot "so it doesn't slide when hover swaps label for controls".
- **Project name:** `min-w-0 flex-1 truncate font-normal text-xs text-secondary-label` (12px, #8f8f8f).
- **Status slot** (:1461-1582): `group/sidebar-status-slot relative ml-auto flex h-5 min-w-8 shrink-0 items-stretch justify-end text-xs`.
  - **Status label** (when `topStatus`): `inline-flex items-center gap-1 font-medium` in the status colour, holding a **16px** lucide icon, the label, and a duration.

    | status | icon | colour | label text |
    |---|---|---|---|
    | preparing / working | `CircleDashed` | `text-info-foreground` (blue-400) | "Setting up", "Goal", or "**Working**" + `WorkingDuration` since the run started |
    | waiting | `CircleDotDashed` | `text-muted-foreground` (#a3a3a3) | "**Waiting**" + duration since the last run completed |
    | approval | `ShieldQuestion` | amber-400 | "Approval" |
    | input | `MessageCircleQuestion` | indigo-300 | "Input" |
    | request | `UserRound` | fuchsia-300 | "Request" |
    | limited | `CircleAlert` | amber-400 | "Limited" |
    | failed | `CircleAlert` | red-400 | "Failed" |
    | woke | `AlarmClock` | amber-400 | "Woke" (a button; hover underlines it; click dismisses) |
    | unread completion | `CircleCheck` | emerald-400 | "Done" |
    | otherwise | none | `text-secondary-label tabular-nums` | **relative time** |

    - **Note: the dashed ring is not animated in this source.** `TopStatusIcon` applies only `shrink-0 size-4` (:531-534), and nothing in sidebar/ or index.css targets `.lucide-circle-dashed`. It is a static lucide `CircleDashed` (8 dashes) for Working and `CircleDotDashed` (alternating dots and dashes) for Waiting. The only moving status indicator is the terminal icon's stepped `status-pulse` (opacity 1→.5 in `steps(6)`, 2s, index.css:332-346).
    - The duration span is `tabular-nums` and **ticks every second** (`useNowSecond`), with all rows sharing one tick. Format (client-runtime models.ts:395-401): `<60s` → `"42s"`, `<60m` → `"12m"`, otherwise `"1h 5m"`.
    - **Relative time** ticks once a minute (`useNowMinute`). `formatRelativeElapsed`: `<60s` → "just now", else `Nm` / `Nh` / `Nd` + " ago". `compactSidebarTimeLabel` strips " ago" and maps "just now" → "now" (:137-151). So the label reads `now`, `12m`, `3h` or `2d`. The timestamp is `resolveThreadActivityTimestamp`.
  - **Hover swap:** the label span carries `group-any-hover/sidebar-row:absolute right-0 opacity-0` (and the same on `has-[:focus-visible]`). The **action strip** (:1526-1580) is `pointer-events-none absolute inset-y-0 right-0 flex items-stretch opacity-0`, and becomes `static opacity-100 pointer-events-auto` on row hover or keyboard focus. `any-hover` is a custom variant gated on `@media (any-hover: hover)` (index.css:11-17).
    - **No transition:** the status-to-actions swap snaps, and because the label goes absolute the slot reflows.
    - A comment explains why focus-visible is used instead of focus-within: "Mouse clicks leave Settle focused".
    - While the snooze menu is open the strip stays pinned visible.
  - Actions, left to right, each in a full-height (20px) slot:
    - Discard draft: `X` 14px, `px-1.5`, only when there is a draft.
    - **Snooze:** `ClockIcon` **12px**, `inline-flex h-full items-center gap-0.5 rounded-md px-1.5 text-xs text-muted-foreground hover:text-foreground`, tooltip "Snooze thread". It opens a `Menu` (side bottom, align end) of presets, each with a `MenuItemDetail` time, then a separator and "Custom…" (:370-431). It is hidden while a turn is queued (a grace gate).
    - **Settle:** `CheckIcon` **14px** + the word "**Settle**", `-mr-1 inline-flex items-center gap-1 rounded-md px-1.5 text-xs text-muted-foreground hover:text-foreground`, tooltip "Settle thread". Pressing starts an "action sweep" (`onPointerDown`), so dragging down across rows settles all of them, and the swept rows show a "✓ Settle" badge.
  - **Jump hints:** while the jump modifier is held, a `Kbd` (`h-5 min-w-5 rounded px-1 text-xs font-medium text-muted-foreground bg-foreground/7%`) shows the ⌘-digit, and all other slot children go invisible and absolute (:161-162).

### Row 2: title (`mt-1 flex min-w-0 items-center gap-1.5`, :1586)
- `min-w-0 flex-1 truncate text-start text-sm` (14px/20px), `font-medium` or `font-normal` as receding dictates, with `dir` auto-detected for RTL.
- While the title regenerates it is `opacity-55`.
- **Truncation is a plain end ellipsis.**
- When the meta line is empty, the provider icons move here (`titleLineIcons`).

### Row 3: meta (`mt-0.5 flex min-w-0 items-center gap-1.5 text-secondary-label text-xs`, :1153-1192)
- **Worktree icon** `FolderGit2` **14px**, `text-icon-muted`, shown only when `worktreePath` is set. Its tooltip reads "Worktree: ~/path (branch)" (ThreadStatusIndicators.tsx:572-604).
- **Branch** in `MiddleTruncate` (ui/middle-truncate.tsx), with `showTitle={false}`:
  - The tail kept is the last `/` segment if it is ≤16 characters, otherwise 10 characters, snapped to a word boundary in `[-_./]` (preferring a shorter tail, minimum 4).
  - No truncation if the length is ≤ tail + 4.
  - The head is `overflow:hidden`. The **tail span carries the "…" via `peer-data-overflow:before:content-['…']`**, so the ellipsis sits flush. JavaScript binary-searches the last whole glyph that fits, clips the partial glyph with `clip-path: inset(-0.25em <slack>px -0.25em 0)`, and slides the tail left by the slack. A ResizeObserver re-fits on width changes.
  - Result: `t3/improv…architecture`.
- Terminal status button (a 14px terminal icon with `animate-status-pulse`), when terminals are running.
- **PR badge** `ThreadPullRequestBadgeControl` rendered as an `InlineButton` (`inline-flex gap-0.5 font-medium hover:underline underline-offset-2`). The content is `contents font-normal text-xs tabular-nums` in the state tone:
  - Icon **12px**: open `GitPullRequestArrow` emerald-300/90; draft `GitPullRequestDraft` zinc-400/80; closed red-300/90; merged `GitMerge` violet-300/90.
  - Then the number in `text-secondary-label` (no "#").
  - With several PRs: a `Layers` icon and the layer count, or `+N`.
  - Click opens the URL (or the right panel). `pointerdown` stops propagation so the card doesn't drag.
  - **Hover popover** (ThreadStatusIndicators.tsx:293-318): `TooltipPopup side=top sideOffset=0 variant=glass`, `w-80` (320px), `pointer-events-auto` so the pointer can move into it and click. It holds a mini list (`flex flex-col gap-1`, scrolls at `--available-height`) of items `flex items-center gap-2 rounded-sm px-1 py-1 hover:bg-accent`: a 12px state icon, `#415` in `font-mono tabular-nums`, and the title `truncate text-foreground/75`. Stacked PRs indent 12px per depth (up to 3) and get a `stack · N` tag at `text-3xs`.
- Diff `+12 −3` in `font-mono` (currently stubbed to null).
- Right cluster `pointer-events-none ml-auto inline-flex gap-1`: a remote machine icon (14px), then the **provider stack** (:183-229).
  - The current provider is `ProviderInstanceIcon size=compact`: a **14px** logo at `opacity-60`, in its brand colour (Claude's orange asterisk).
  - Earlier providers after a handoff show at 12px `opacity-35 grayscale`, `gap-1.5`, with the current one on `z-10`.
  - When there are several instances of the same driver, an instance badge (`h-3 min-w-3 text-5xs`) sits at the bottom-right corner.

### Hover card (whole row, :231-368)
- The whole row is a Tooltip trigger. Its popup is `ThreadHoverCardPopup`: **side right, align start, sideOffset 4**, glass, `max-w-80`.
- Body: `flex flex-col gap-2 px-1 py-2`.
  - Title: `text-xs leading-tight font-medium text-foreground truncate`.
  - Then a grid `gap-1.5 pl-0.5 text-xs text-muted-foreground` of 12px icon + `text-foreground/75` lines: project (favicon), environment (machine icon), branch (GitBranch + middle-truncate), branch-mismatch warning, model (grayscale provider icon + "Opus 4.x · Instance"), "Handed off from …", "N terminal processes running", and "Usage limit reached" or "Error occurred".
  - Footer: the PR mini list above a `border-t border-border/60` rule.
- It is disabled while dragging or while the snooze menu is open.

### Tooltips (ui/tooltip.tsx, Sidebar.tsx:3311-3316)
- The list's `TooltipProvider` uses **`delay=150`, `closeDelay=0`, `timeout=400`**: hover 150ms to open; once one tooltip has shown, the next one within 400ms opens at once.
- Popup: `rounded-md text-xs`, viewport `px-2 py-1`, `max-w-80 leading-snug text-balance`.
  - Enter and exit: `transition-[scale,opacity]` (150ms), from and to `scale-98 opacity-0`, `origin-(--transform-origin)`.
  - The positioner itself transitions top/left/transform, so a tooltip slides between triggers.
  - Default variant: `border bg-popover shadow-md/5`, with an inner 1px highlight line (`before: shadow 0 -1px white/6%` in dark).
  - Glass variant (`dropdown-glass`, index.css:454): `popover` at 80% over a blur(16px dark) saturate(1.08) backdrop, a 1px border of foreground at 10%, and `shadow-xl shadow-black/25`.
- Scrolling dismisses hovered tooltips (`TooltipScrollDismissArea`).

### Click, keyboard, context menu
- Row: `role=button tabIndex=0`, `aria-current` when active, and an sr-only title.
- Enter or Space opens the thread (:847-861).
- **F2** or **double-click** renames; double-click ignores modifier clicks and clicks on buttons, links or inputs (:862-876). The rename input is `min-w-0 flex-1 rounded-sm border border-input bg-card px-1 text-sm font-medium focus:border-foreground`. It selects all on focus; Enter commits; Esc cancels; blur commits (:593-631). IME composition is guarded.
- Tree navigation (lib/sidebarTree.ts):
  - ↑/↓/Home/End move **focus without opening**, VS Code style, using `scrollIntoView({block:"nearest"})`.
  - ←/→ go to the parent header and back.
  - The ContextMenu key and Shift+F10 open the menu.
  - Focus follows a row that moves shelves after it remounts.
- Right-click opens the **native** menu (`api.contextMenu.show`, threadActionMenu.logic.ts:120-220). Items: Find in thread; New thread on branch; Pin/Unpin; Settle/Un-settle; Snooze ▸ presets / Custom…, or Wake; — separator —; Rename thread; Regenerate title; Mark unread; Share with guest; Filter by project; Turn into project; Auto-settle ▸; Delete. With a multi-selection the items carry counts ("Settle (3)").
- Selection: cmd/shift-click multi-selects (`threadSelectionStore`), and selected rows take `bg-sidebar-row-selected`.
- **Route reveal** (sidebar/routeReveal.ts:33-46): on every route change and on mount, the active row scrolls into view (`block: nearest`) once, unless it is under the pointer, "just clicked, so in view". Revealing a thread can also widen the project scope.

---------------------------------------------------------------------------
## 4. Slim rows (Snoozed and Settled shelves)

SidebarThreadRow.tsx:1203-1400.
- Layout: `flex h-9 items-center gap-2.5 px-2.5` (36px tall).
- Project favicon `size-4` at `opacity-40 grayscale`, back to full colour on hover or focus (`transition-opacity`).
- Title `text-sm truncate`, coloured `text-secondary-label` when receded, otherwise `secondary-label/70` (muted for unread, foreground for active, woke, input or request). Everything in the row goes `text-secondary-label` unless hovered: `[&:not(:hover):not(:focus-within)_*]:text-secondary-label`.
- Right slot `relative ml-auto flex h-6 min-w-8 justify-end`:
  - At rest: the settled-relative time (`text-xs tabular-nums text-secondary-label`), or for a snoozed row a blue `AlarmClock` and the wake time.
  - On hover the time fades (`transition-opacity` → 0) and an absolutely placed action fades in (`opacity-0 → 100`, transition-opacity 150ms): `Undo2` 14px "Un-settle" or `AlarmClockOff` 12px "Wake".

---------------------------------------------------------------------------
## 5. Motion: how cards move Working → Settled, enter and leave

**Hand-rolled FLIP with the Web Animations API** (Sidebar.motion.ts). No framer-motion.
- `motionTiming = { duration: 150, easing: "ease-out" }` (:1).
- After each layout-affecting commit (`useLayoutEffect`, Sidebar.tsx:2007-2012) the controller compares each direct child's `offsetTop` with the previous frame:
  - **Moved** rows animate `translateY(prev − new) → 0` (:101-118). An interrupted animation's remaining offset carries into the next one (`remainingOffset`), so a rapid re-sort never jumps.
  - **Entering** rows animate `opacity 0, translateY(travel) → opacity 1, translateY(0)`. `travel` is the delta of the nearest moved neighbour above ("riding"), or else **−min(height, 40px)**, a small drop-in from above (:189-211).
  - **Exiting** rows: the node is **cloned** (ids, testids and data-thread-item removed; `inert`, `aria-hidden`), placed absolutely at its old box, and animated to `opacity 0, translateY(+min(h,40) or the riding delta)`, then removed (:40-89).
  - Safety valves: if more than **40** rows fade in one update, the update snaps (`MAX_FADED_ROWS_PER_UPDATE`); `prefers-reduced-motion` snaps; and it doesn't run on the first paint, on shelf toggles, or while dragging.
- So **settling a card**: the card is removed from Active (its clone fades and slides down ~40px) and a slim row enters in the Settled shelf. Since shelves start collapsed, usually only the Settled count changes. Rows below slide up into the gap over 150ms ease-out.
- Drag and drop (dnd-kit): rows shift with dnd-kit's default 200ms sorting transition, only while sorting (`animateSidebarLayoutChanges`, Sidebar.logic.ts:169). The drop target shows a dashed placeholder (`h-9 rounded-md border border-dashed border-sidebar-foreground/25 text-xs`, or primary/40 with a primary/5 fill as the target) and boundary labels ("Pinned"/"Active" plus a 1px rule) revealed with a 200ms `step-end` visibility keyframe (index.css:2423-2436). The dragged row's destination badge ("✓ Settle", "Pin", "Wake") is `h-5 rounded-sm border border-primary/40 bg-primary/10 px-1.5 text-2xs font-medium text-primary`.
- The undo notice fades in and out over 200ms.

---------------------------------------------------------------------------
## 6. Empty states and search

- No threads (Sidebar.tsx:3650-3677): `flex flex-col items-center gap-2 px-2 py-6 text-center text-xs text-muted-foreground/60`, reading "No threads yet", "No threads in {project} yet" or "No threads in these projects yet".
- No projects: "No projects yet" plus a button `rounded-md border border-sidebar-border px-2.5 py-1 text-2xs font-medium` with a 12px Plus and "Add project", hover `bg-sidebar-row-hover`.
- Search replaces the list with a `role=listbox` of `SidebarSearchResultRow`s:
  - Row: `min-h-9 px-2.5 py-1 gap-2.5 rounded-md text-sm`; the highlighted row is `bg-sidebar-row-active`.
  - Contents: the favicon, the title, the matching PR number, then the time in a fixed `w-8 text-right text-xs text-muted-foreground/55`.
  - A match excerpt can appear under the row. The mouse moving over a row highlights it.
  - Keys: ↑/↓/Enter/Esc. The empty result reads "No threads found" or "Searching thread messages…".

---------------------------------------------------------------------------
## 7. Ferrite today (for comparison)

Ferrite's nav is deliberately a **Claude/Codex tree**, not T3 cards. See the header doc in `crates/ferrite-cockpit/src/nav.rs:1-50` and the row doc at :1517-1525: "Every row is one quiet line … the title and nothing else … where a Thread works and what its PR says live in its Pane and the panel."

Key tokens (`crates/ferrite-ui/src/theme.rs`):

| token | value |
|---|---|
| `SIDEBAR` | `#212121` |
| `SIDEBAR_HOVER` | `#292929` |
| `SIDEBAR_SELECTED` | `#414141` |
| `NAV_WIDTH` | 256 |
| `NAV_ROW_H` | 30 |
| `NAV_ROW_GAP` | 4 (34px pitch) |
| `NAV_ROW_PAD_X` | 10 |
| `FS_NAV` | 13 |
| `FS_META` | 13 |
| `R_CONTROL` | 6 |
| `INK_STRONG` | `#efefef` |
| `INK_SUBTLE` | `#9c9c9c` |
| `RUNNING` | green `#93cf8c` |
| `ATTENTION` | yellow |
| `UNREAD` | link blue `#7aa8ee` |
| `NAV_BRANCH_MAX_CH` | 24 (end truncation) |
| `MOTION_FOLD_MS` | 150 |

Motion catalog (`crates/ferrite-ui/src/motion.rs:11-48`): row hover snaps; nav folds animate height and opacity over 150ms ease-out; chevrons turn over 60ms; tooltips appear after gpui's 300ms delay with a 120ms entrance; **no list-reorder/enter/exit motion**.

The Inbox view (`cockpit.rs:13772-13870`) already has Inbox, a `Working · N` fold and Snoozed, with hover row actions including Snooze (`activity_row_actions`, cockpit.rs:13819). The data the rows carry (`nav::ThreadRow`, nav.rs:488-500) is only `thread, name, status, project, selected, tail`.

## 8. Concrete gaps (Ferrite vs T3 default view)

1. **Row anatomy.** T3: a 3-line, 78px card (+4px padding, 1px gap). Ferrite: one 30px line, title plus one mark (`thread_row_with_title`, nav.rs:1526-1593). Adding the card means adding `branch`, `worktree: bool`, `pr: Option<(number, state, title)>`, `provider`, `activity_at` and `working_since` to `nav::ThreadRow` (nav.rs:488); the cockpit assembles these per frame.
2. **Project badge.** T3: a 16px monogram tile, 25% radius, hue at 14% fill, 8.25px/700 mono text stretched to 12px, with a deterministic 17-hue hash. Ferrite has none: Project headings are label-only by design (`project_section`, nav.rs:1127; there is `NAV_AVATAR` 18px for the account only).
3. **Status words + live duration.** T3: a 16px icon + `Working 12m` in `text-xs font-medium`, coloured (blue Working, grey Waiting), with the duration ticking every second (`42s` → `12m` → `1h 5m`); otherwise a compact age (`now/12m/3h/2d`) per minute. Ferrite: a mark only (`state_mark`, nav.rs:1595); idle rows say nothing and have no age. (`NavTail::Working("1m04s")` exists but the tree doesn't show it.)
4. **Colour semantics differ.** T3 Working is *blue* and receded; Ferrite `RUNNING` is *green* (theme.rs:486). T3 Done = emerald `CircleCheck` "Done" text; Ferrite = a 5px blue dot.
5. **Receding busy rows.** T3 dims working/waiting cards to 70% opacity with a normal-weight, 75%-ink title, restored on hover with a 150ms opacity fade. Ferrite: working titles are full `INK_STRONG` (`title_face`, nav.rs:1484-1492); only Done is bolded.
6. **Hover action strip.** T3 swaps status for `[🕒 snooze 12px][✓ 14px + "Settle"]`, each a 20px-high `px-1.5` muted→foreground text button, plus press-and-sweep across rows. Ferrite tree rows get a single `⋯` (`row_action`, nav.rs:1109). Snooze exists only in the Inbox rows.
7. **Structure and anchoring.** T3 is a flat, recency-sorted card list; the Working/Snoozed/Settled shelves are pushed to the **bottom** by `mt-auto` on the first shelf header, start collapsed and are persisted. Ferrite is a Project tree with `Show more` after 5 rows (nav.rs:158), top-anchored; its Inbox has a `Working · N` fold heading (cockpit.rs:13853-13858).
8. **Shelf header styling.** T3: 32px, `text-xs font-medium` muted, an invisible flex rule, the count right-aligned `font-normal tabular-nums`, then a 12px chevron (down open, right closed) rotating over 150ms. Ferrite `fold_heading` (nav.rs:1063): a 26px heading, the caret right after the label, the count joined into the label ("Working · 3").
9. **Fold animation is opposite.** T3 shelves expand and collapse **instantly** (FLIP suppressed on toggle), with only the chevron rotating at 150ms. Ferrite tweens fold height over 150ms (`fold_body`, motion.rs catalog) and the chevron over 60ms.
10. **List motion (biggest feel gap).** T3 FLIPs every re-sort and section move: 150ms ease-out translate, enter fade from −min(h,40)px, exit via a fading clone travelling +min(h,40)px, interruption-safe, capped at 40 rows. Ferrite has nothing like it; rows jump. A gpui port would need per-row previous-y tracking keyed by ThreadId plus a `Tween` on a y offset, and an exit "ghost" painted from the last frame's row.
11. **Branch truncation.** T3 middle-truncates (`t3/improv…architecture`), keeping the last path segment (≤16 characters) or a word-snapped 10-character tail. Ferrite `short_branch` end-cuts at 24 characters (nav.rs:1226-1236). Titles: T3 uses an end ellipsis; Ferrite fades the cut (`fitted` + `line_fade`, nav.rs:603-638). That fade is a fine choice to keep.
12. **PR chip + popover.** T3: a 12px state-tinted PR glyph + the number in `#8f8f8f`, hover-underlined. Its glass popover is 320px with interactive rows `#415 title` (mono number, 75% title). Ferrite shows no PR in the nav.
13. **Provider logo.** T3: a 14px brand-coloured logo at 60% opacity, right of the meta line, with handoff history in grey. Ferrite: none in the nav.
14. **Row hover card.** T3: a rich glass card to the right of a hovered row after 150ms (project, branch, model, handoff, errors). Ferrite tree rows: none found in `thread_row_with_title`.
15. **Tooltip timing.** T3: 150ms delay, 0 close, 400ms warm window, scale .98 + fade 150ms, and the positioner glides between triggers. Ferrite: gpui's 300ms delay, a 200ms warm window, 120ms in, no exit (motion.rs:28).
16. **Surface values.** T3 dark sidebar is `#000` with 3.5% grain, hover ≈ white 8%, active 11%, selected 7%. Ferrite `#212121` / hover `#292929` / selected `#414141` is a heavier selected step (theme.rs:252-254). T3 cards have no border or fill at rest.
17. **Header and search.** T3 has an always-visible inline search field (32px, rounded 6px, hover-filled) with 28px filter/add-project/compose buttons beside it. Ferrite puts bell/search/toggle icon buttons in the head (`head`, nav.rs:818-846), and search opens the palette.
18. **Footer.** T3: a row of 32px icon buttons (Settings, PRs, Usage), the undo notice filling the middle ("Settled, Undo ⌘Z", 11px, 200ms fade), and the update arrow at the right. Ferrite: an account avatar + name + version + gear (`foot`, nav.rs:1983).
19. **Titlebar art.** T3 draws the nightly/dev starry SVG (80px, masked fade). Ferrite has no equivalent.
20. **Keyboard.** T3: arrows move focus without opening, ←/→ jump between header and parent, F2/double-click rename, ContextMenu/Shift+F10, ⌘-digit jump hints replacing the status slot. Ferrite has nav_keys (`cockpit/nav_keys.rs`) and rail digits; check F2, double-click rename and in-row jump hints.
21. **Empty states.** T3: centred `text-xs` at 60% muted, "No threads yet", "No threads in X yet", or "No projects yet" plus an outlined "Add project". Ferrite: `empty_filter` (nav.rs:1802) and the Inbox's "Nothing needs you".
