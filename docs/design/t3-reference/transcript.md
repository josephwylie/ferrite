# T3 Code transcript: exact rendering spec, and gaps against Ferrite

Source: pingdotgg/t3code at `bcee0f5d` (paths below are relative to `apps/web/src/components/chat/` unless they start with `packages/` or `index.css`).
Ferrite paths are relative to the repository root.

Tailwind v4 scale used throughout: spacing 1 = 4px; `text-sm` 14px/20px (overridden to `leading-relaxed` = 1.625, so 22.75px); `text-xs` 12px/16px; `text-2xs` 11px (index.css:183); `text-3xs` 10px; `size-3` 12px, `size-3.5` 14px, `size-4` 16px, `size-6` 24px; `rounded-md` 6px, `rounded-lg` 8px, `rounded-xl` 12px, `rounded-2xl` 16px. A default `transition-*` is 150ms `cubic-bezier(0.4,0,0.2,1)`.

---

## 0. Theme values (dark, the default the screenshots show)

From `index.css:1105-1230` (the `:root` block, `@variant dark`):

| Token | Dark | Light |
| --- | --- | --- |
| `--background` (timeline ground) | `#000` | zinc-25 |
| `--foreground` | neutral-100 (`#f5f5f5`) | zinc-900 |
| `--muted-foreground` | neutral-400 (`#a1a1a1`) | zinc-500 |
| `--secondary-label` (work-log labels) | = muted-foreground | = muted-foreground |
| `--icon-muted` (work-log icons) | = muted-foreground | = muted-foreground |
| `--border` | white 8% | zinc-200 |
| `--muted` | white 3% | zinc-50 |
| `--accent` (hover fill) | white 4% | zinc-100 |
| `--message-surface` (user bubble) | `color-mix(neutral-950 92%, white)`, about `#1e1e1e` | `color-mix(zinc-200 55%, zinc-100)` |
| `--info` | blue-500 | blue-500 |
| `--tool-error-icon` | `#fca5a5` (used at /40) | `--error` |
| links in prose | `--info-foreground` = blue-400 | blue-700 |

`--contrast-*` variants (index.css:1630-1712) are `color-mix` of the token at `--appearance-contrast-base` (100%) toward `--appearance-contrast-target` (white in dark) by `--appearance-contrast-boost` (0%), so by default they equal the plain tokens.

Fonts (index.css:170-174): sans `-apple-system, BlinkMacSystemFont, "Inter Variable", "Segoe UI", system-ui`; mono `ui-monospace, "SF Mono", Menlo, ...`. Code inside chat uses `--font-size-code` (falls back to `--text-2xs`, 11px).

## 1. Column and canvas

- Lane: `.chat-content-lane { margin-inline:auto; width:100%; max-width: var(--chat-content-max-width) }` (index.css:952-957).
- `--chat-content-max-width: 46rem` = **736px** by default (index.css:131). The "Chat width" setting gives `wide` = **72rem (1152px)**, `full` = 100% (index.css:2289-2297). The screenshots' "wide and generous" column is probably `wide`.
- Scroll container side padding: `0.75rem` (12px) below 640px, **`3rem` (48px) at 640px and up**, plus the preview mini-player lane insets (index.css:2551-2577). `chatCanvasLayout.ts:27-29`: `padding = 48, maxChatWidth = 768, minChatWidth = 640` for the canvas width probe.
- Top of the list: a spacer `h-3 sm:h-4` (16px), or `h-[var(--workspace-titlebar-scroll-fade-height)]` (1.5rem) when the top fade is on (MessagesTimeline.tsx:157-160).
- Top fade (`topbar-scroll-fade`, index.css:469-509): a 1.5rem mask, `transparent 0% → 10% at 10% → 30% at 24% → 58% at 42% → 82% at 62% → 96% at 82% → black`. The scrollbar lane is kept opaque.
- Bottom: a composer-height spacer plus `h-3 sm:h-4` (MessagesTimeline.tsx:183-187).
- `scrollbar-gutter-both`, `overscroll-y-contain`, `overflow-anchor:none`. Virtualised by LegendList (`estimatedItemSize={90}`). While the agent works it follows the end with `animated: true` (it smooth-scrolls to the tail), and otherwise does not animate (MessagesTimeline.tsx:192-205, 1310-1318).

## 2. Row spacing (vertical rhythm)

`TimelineRowContent` (MessagesTimeline.tsx:1440-1470) gives each row's **bottom** padding:
- work-log rows (work, work-live, work-toggle, thinking) and subagent groups: none of their own. They use `WorkLogBlock` instead, which is `pb-2` (8px) standalone, **`pb-0` when the next row is also a work-log row** (`continuesWorkLog`), `pb-0` for an expanded group's header, and `pb-1` for the group's content (WorkLog.tsx:14-36). Consecutive tool rows therefore touch: each is a `min-h-6` (24px) line.
- turn-fold and working rows: `pb-1.5` (6px).
- a user message followed by another of yours: `pb-1` (4px).
- an assistant message without meta, worktree-setup, event, attempt-fold, html-render, mcp-app: `pb-2` (8px).
- everything else (a user message, an assistant message with its meta): **`pb-4` (16px)**.

## 3. "Working for 11m 24s" (live turn header)

- Placement: the row is inserted at `activeTurnHeaderIndex = lastResponseBoundaryIndex + 1`, i.e. **directly under the prompt that started the turn, at the top of the turn**, not at the tail (MessagesTimeline.logic.ts:1622-1624, 1820-1823).
- Markup (TimelineWorkGroupRows.tsx:278-310):
  ```
  <div className="border-b border-border/60 pb-2 pt-1">
    <div className="flex h-6 min-w-0 items-baseline gap-2 px-1 text-sm leading-relaxed text-muted-foreground tabular-nums">
  ```
  The hairline is `border-b` in `--border` at 60% (white 8% × 0.6, about 4.8% white). There is 4px above the text and 8px between the text and the line. The row's own `pb-1.5` follows it.
- Text: `Working for <WorkingTimer/>`. Before the turn's start is known it reads `Working...`. While compacting it reads `<Minimize2 size-3> Compacting for <timer>`, and while setting up a worktree `Setting up worktree…`. Only those last two shimmer. **"Working for" itself does not shimmer.**
- Timer (TimelineWorkGroupRows.logic.ts:3-20): under 60s `${s}s`, otherwise `formatDuration` (packages/shared/src/orchestrationTiming.ts:22-37), which is `Xh Ym Zs` with zero parts dropped and single spaces between them (`11m 24s`, `5m`, `1h 2m`).
- Ticking: `tickTextContent(format, true)` (lib/tickTextContent.ts) writes `textContent` straight into the DOM on a shared `secondTick`, so the timeline never re-renders. The span is `tabular-nums`.

## 4. "Worked for 24m 55s ›" (turn fold) and when Working becomes Worked

The logic is `deriveTurnFolds` (MessagesTimeline.logic.ts:1004-1221), called with `unfoldedRunIds = activeVisualResponseRunIds ∪ failedRunIds` (1470-1500).
- A run is **not** folded while it is active, failed, interrupted (it has `run_interrupt_*` events), while any of its messages is still streaming, or in "zen" visibility.
- Once the run settles, every entry of that turn is hidden **except**: the terminal assistant message (the final answer), compaction rows, persistent resource cards, live subagent cards, notifications, question/answer entries, and superseded-reply entries. Work after the terminal message is hidden only when it is a completed, non-failed work entry. If nothing but compaction would hide, there is no fold.
- **Collapsed by default**: `expanded: input.expandedRunIds?.has(runId) ?? false` (1847). The open state is per run and is remembered in the timeline position cache.
- Label (1180-1188): `Worked for ${formatDuration(ms)}`, or `Worked` with no duration. For the latest run, if interrupted: `You stopped after ${d}` or `You stopped this response`. A steer splits the work: each later segment gets its own fold, `Continued for ${d}` (1205-1219).
- Duration: if the latest run has `startedAt` and `completedAt`, it is their difference. Otherwise it runs from the turn's start boundary (the prompt's time) to max(terminal message `updatedAt`, end of the last entry).
- There is **no minimum duration**: a 2s turn still folds into `Worked for 2s`. formatDuration below 10s shows one decimal (`4.1s`), and below 1s shows ms.
- The fold row sits at the turn's anchor, where Working was (the first entry after the prompt). So **"Working for" is replaced in place by "Worked for"** and the final answer shows below it.
- Markup (MessagesTimeline.tsx:1621-1652):
  ```
  <div className="group/timeline-row border-b border-border/60 pb-2 pe-0.5 pt-1">
    <div className="relative flex items-center gap-1">
      <button className="flex cursor-pointer select-none items-center gap-1 rounded-md px-1 text-sm leading-relaxed text-muted-foreground tabular-nums transition-colors hover:text-foreground ...">
        <span>{label}</span>
        <MorphIcon className="size-3.5" icon={expanded ? ChevronDown : ChevronRight}/>
      </button>
      <TimelineRowTimestamp className="ms-auto" .../>
  ```
  The chevron is 14px, **after** the label, 4px gap, and its glyph **morphs** between right and down (`morphicons`) rather than rotating. On hover the label and chevron go muted → foreground over 150ms. The timestamp is right-aligned and appears only on row hover (§13).
- Expanding a fold whose work was deferred server-side shows `WorkHistorySkeleton` (three `Skeleton` pill bars, `h-2.5` at widths 2/5, 3/5 and 1/3, with 12px icons; `space-y-2 px-1 pb-1 pt-3`). A load failure appends ` · Couldn't load work. Click to retry.`

## 5. Work-log row anatomy (every tool, thought or update line)

`WorkLogLine` (WorkLog.tsx:53-81):
```
<div className="flex min-h-6 min-w-0 gap-1.5 text-sm leading-relaxed select-none items-center">
  <div className="flex min-w-0 flex-1 items-baseline gap-1.5">
    <span className="relative w-6 shrink-0 leading-0">           ← 24px icon gutter
      <span className="inline-block size-6 overflow-hidden align-[calc(0.5cap_-_0.75rem)]">
        <span className="flex size-full items-center justify-center">{icon}</span>
    <div className="min-w-0 flex-1 text-secondary-label truncate [&_*]:whitespace-nowrap">{label}</div>
  {trailing}
```
- Row wrapper (WorkLog.tsx:86-93): `group/timeline-row relative w-full min-w-0 rounded-md px-0.5 text-left transition-colors`, `py-0.5` standalone and **`py-0` inside a group list**. Interactive rows also get `cursor-pointer` and an inset ring on focus. **No hover background.**
- Geometry: 14px text on a 22.75px line, row at least 24px, 2px side padding, a 24px icon box holding a 16px icon, 6px gap to the label. The label is a single line ellipsised with `truncate`. Text is not selectable (`select-none`); details are.
- Icon colour (TimelineWorkEntryRow.tsx:357-368 and TimelineToolActivityIcon.tsx:86): `text-icon-muted` (muted-foreground) with **`opacity-70`**, so the effective value is about `#a1a1a1` at 70% on black (about `#717171`). Lucide default stroke 2 (on a 24 viewBox, 1.33px at 16px).
- Label colour: tool-like entries `text-secondary-label` (muted-foreground). Non-tool entries (updates) `text-foreground/80`. Warnings and severe failures are `font-medium text-warning` or `text-destructive`.
- Trailing, in order: optional `Open chat` (text-sm text-primary) or `Open subagent` inline button; a failure `XIcon size-3` in `text-tool-error-icon/40` when the icon cannot be tinted; the **hover timestamp**; then the **chevron slot**: `flex size-4` (16px) holding `ChevronRightIcon size-3 text-icon-muted opacity-70 transition-transform duration-200`, `rotate-90` when open. The slot is `invisible` (space kept) when the row cannot expand (TimelineWorkEntryRow.tsx:496-510).
- A failed tool tints its icon `text-tool-error-icon/40` (`#fca5a5` at 40%), with the role "Tool call failed". Errors with `status failed` show `CircleAlertIcon` in destructive, a `font-medium` label, the timestamp always visible, and the message on its own line: `ms-7 whitespace-pre-wrap py-1 text-sm text-foreground/80`.

### Icon set (lucide-react) — TimelineToolActivityIcon.tsx, TimelineWorkEntryRow.logic.ts:12-53
| Kind | Icon |
| --- | --- |
| command | `TerminalIcon` (`>_`) |
| edit / file change | `SquarePenIcon` (pencil-in-square) |
| read | `EyeIcon` |
| code search | `SearchIcon` |
| web search | `GlobeIcon` |
| reasoning | `BrainIcon` |
| mixed / update / dynamic tool | `HammerIcon` |
| other tool | `WrenchIcon` |
| subagent / agent tool | `BotIcon` |
| tone tool / background task | `ZapIcon` |
| question / approval | `MessageCircleIcon` |
| todo list | `ListTodoIcon` |
| PR tools | PullRequestGlyph |
| thread create / system | `T3Wordmark` |
| JS REPL | `Code2Icon` |
| browser | custom `BrowserAppIcon` (stroke 1.9) |
| device | `SmartphoneIcon` |
| error | `CircleAlertIcon` |
| info | `CheckIcon` |
MCP/integration tools can show a website favicon (32px source) or a themed logo in a `rounded-xs bg-background` tile, `dark:opacity-70`. A native-app icon is drawn at `size-5`.

## 6. Grouping rules

`deriveMessagesTimelineRows` (MessagesTimeline.logic.ts:1928-2063):
- **Breakers.** A run of consecutive `work` entries forms a group until one of these: a non-work entry; a compaction; an error or failed item; a `runtime.error`; a `system_notice`; a `notification`; a `todo_list`; a question/answer; a pending approval; an entry in the live tail; a fold anchor or hidden entry; **a different `runId`**; **a different attempt**. Each breaker becomes its own standalone row.
- **Hidden entries.** Entries in neutral status are not shown (`workEntryIsVisibleInGroup`, 271-282). Reasoning is visible only when it has text.
- **Group shape:**
  - one visible tool-like entry: **not a group**. It is a plain `work` row (an entry row with its own chevron). Its label is `singleToolCallLabel`, except an edit, which uses the summary text (`Changed 1 file`).
  - one non-tool entry: a toggle row whose summary is that entry's label.
  - otherwise a **`work-toggle` summary row** (collapsed by default, `expandedWorkGroupIds` empty) whose icon follows `toolGroupSummaryKind`. All one kind gives that kind's icon, mixed gives `hammer`, all reasoning gives `brain`.
  - When expanded, a second row `${groupId}:details` holds the list.
- **Summary text** (`summarizeToolGroup`, packages/client-runtime/src/work-log/presentation.ts:558-645):
  - Only reasoning: `Thought` or `Thought (×N)`. Otherwise reasoning is excluded from the count.
  - Entries are bucketed by action. Labels (463-498): `Ran N command(s)`, `Changed N file(s)` (N = **unique changed files**, plus edits without file info), `Read N file(s)`, `Searched code N time(s)`, `Searched the web N time(s)`, `Used browser N time(s)`, `Used N tool(s)`, `Received N update(s)`, `Created N thread(s)`, PR variants.
  - **At most two** buckets are named. Priority 0 (commands, edits, thread/queue/project actions) beats 1 (reads, searches) beats 2 (other, update). The two are then kept in first-appearance order.
  - Tools from an integration source lead the summary: `Used Linear integration`, `Used X and Y integrations`.
  - The rest become `Performed N other action(s)`.
  - Every label after the first is lower-cased at its first letter. Two labels are joined with ` and ` (**`Ran 4 commands and changed 10 files`**). Three or more get an Oxford comma: `A, B, and C`.
  - Failure is never in the text. `hasFailure` (the last tool failed) only feeds aria and the icon tint.
- **Group header row** (`WorkGroupHeader`, TimelineWorkGroupRows.tsx:608-637): a `WorkLogButton` with icon, label, and a trailing timestamp. **There is no chevron on a group header.** The chevrons the user sees are on entry rows.

## 7. Expanded group list (capped scroller with fades)

`ExpandedWorkGroupEntries` (TimelineWorkGroupRows.tsx:125-274):
- A nested, virtualised **LegendList** (`estimatedItemSize=24`, `drawDistance=240`, `recycleItems`) of `SimpleWorkEntryRow`, each a 24px `py-0` row.
- `style={{ maxHeight: calc(min(18rem, 50dvh) + ${expandedContentHeight}px) }}`. The cap is **288px** (or half the viewport), and it grows by the extra height of any entries opened inside it, so an opened entry does not get trapped.
- Classes: `scrollbar-gutter-stable scroll-py-6 overflow-x-hidden rounded-md`, focusable `tabIndex=0`, `role=region aria-label="Tool calls"`.
- **Fades** (`getVirtualizedScrollFadeClassName`, components/ui/scroll-area.tsx:5-20). `--fade-size: 1.5rem` (24px) as a mask:
  - top-only: `linear-gradient(to bottom, transparent, black 24px)`
  - bottom-only: `linear-gradient(to bottom, black calc(100% - 24px), transparent)`
  - both: both ramps.
  A fade appears only when there is more content past that edge (`scrollTop > 1` or `distanceFromEnd > 1`). The scrollbar lane stays unmasked (`virtualized-scroll-fade`, index.css:511-524). This is the "last row faded at the bottom edge".
- **Follow**: while it sits at the bottom and entries are appended (same prefix), it pins to the end without animation (`shouldFollowWorkGroupAppend`). Its scroll position (row id plus offset) is remembered per group across virtualisation and thread switches.
- **No open/close animation.** Expanding inserts a separate list row; the header and the content are two rows of the outer list.

## 8. Entry rows inside a group (and alone)

`WorkEntryLogRow` (TimelineWorkEntryRow.tsx:174-568):
- Label: `workEntryDisplayLabel` (MessagesTimeline.logic.ts:137-182). Command text uses `commandDisplayText`. For reads it is `formatReadToolLabel(path, more)`. Edits use the relative path, or `path +N more`. The fallback is the tool title capitalised.
- **Thought entry, collapsed**: `BrainIcon` with the reasoning flattened to one line by `remarkThoughtPreview` (markdown flattened, math kept), truncated. Colour is `text-secondary-label`.
- **Thought entry, expanded**: the label changes to **`Thought`** (`Thinking` while in progress). The body is `ReasoningTraceContent` → `WorkLogDetails kind="text"`:
  `ms-7 flex max-h-96 flex-col gap-3 overflow-auto px-0.5 py-1 select-text` (WorkLog.tsx:157-163). That is a **28px indent** (past the 24px icon and the 6px gap), at most **384px** tall with its own scroll, 12px between paragraphs. The ChatMarkdown inside is `text-foreground`: **full-brightness paragraphs**, `lineBreaks`. While it streams it follows the live edge (4px slack).
- **Edit entry, expanded** → `WorkLogDetails kind="panel"` (`mt-0.5 mb-1.5 pb-2`, not indented), then `V2ItemInspector` (V2ItemInspector.tsx:276-320):
  `space-y-2 text-xs`, then `flex flex-wrap items-center gap-2`: `<span class="font-mono text-muted-foreground">relative/path</span>`, `+N` (text-diff-addition) `-N` (text-diff-deletion), and a **`Button size="xs" variant="outline"` reading "Open diff"** (h-6, text-xs, `px-[7px]`, `bg-popover` border-input, dark `bg-input/32`). Below that, if present, a `ul.space-y-1 font-mono text-muted-foreground` of `operation old → path`.
- **Command entry, expanded**: `ShellCommandBlock` (syntax-highlighted shell, `text-foreground/85`, words wrap anywhere), then `CommandOutputPanel` (`max-h-80` = 320px, `text-muted-foreground`, ANSI colours, follows the bottom with 8px slack, `Earlier output not shown`, `Loading output…`, `No output.` in italic muted), then `exit N` in destructive. Mono class: `font-mono text-(length:--font-size-code,var(--text-2xs)) leading-relaxed whitespace-pre-wrap break-words select-text` (11px).
- Fallback body: `<pre class="max-h-64 cursor-text overflow-auto whitespace-pre-wrap break-words font-mono text-secondary-label text-[11px] leading-relaxed select-text">`, max 256px.
- An expanded reasoning item inside the inspector: `rounded-md border border-border/45 bg-muted/15 p-2 italic text-muted-foreground`.

## 9. Live rows (running tool, thinking) and the shimmers

- **Live tail** (`work-live`, MessagesTimeline.logic.ts:1636-1707): the trailing in-progress entries of the active run collapse into one row that shows the **latest running entry**: `liveWorkEntryLabel` = `Running sed` / `Ran sed` / `Failed sed` / `Declined …` / `Stopped …`. The verb comes from `commandProgramName(command)` (the program only, e.g. "sed"). The row is a button that expands the live group.
- If there is a recent non-empty reasoning entry and the row is not expanded, a **thought line** sits above it: brain icon plus `liveThoughtLine` (the first `**bold**` heading, or else the first sentence), `line-clamp-4` (TimelineWorkGroupRows.tsx:509-552; presentation.ts:674-686).
- **Thinking row** (the tail when nothing visible runs): `LiveActivityRow label={liveAnswerPreview ?? "Thinking"} iconName="brain" active shimmer` (TimelineWorkGroupRows.tsx:361-387).
- `LiveActivityRow` container: `relative min-h-6 w-fit max-w-full min-w-0 overflow-hidden rounded-md text-sm leading-relaxed`. Its width hugs the text, so the shimmer only sweeps the words.
- **Shine A, `live-tool-shine`** (index.css:551-576), applied to the label of a running tool row:
  ```
  color: transparent;
  background-image: linear-gradient(to right, transparent, var(--contrast-foreground), transparent),
                    linear-gradient(var(--contrast-secondary-label), var(--contrast-secondary-label));
  background-size: 4.5rem 100%, 100% 100%; background-repeat:no-repeat; background-clip:text;
  animation: live-tool-shine 2.2s steps(30) infinite;
  @keyframes live-tool-shine { from {background-position:-4.5rem 0, 0 0} to {background-position: calc(100% + 4.5rem) 0, 0 0} }
  ```
  A **72px-wide band** of foreground white moves across text in the muted label colour. It travels from 72px before the text to 72px past it in **2.2s in 30 discrete steps** (about 13.6fps, deliberately stepped), and loops forever. It is applied only under `prefers-reduced-motion: no-preference` and `forced-colors: none`.
- **Shine B, `ActivityShimmerOverlay`** (TimelineRowParts.tsx:44-55, index.css:586-660), used by the Thinking row and active compaction. A copy of the content in `text-foreground` (icon included, `text-foreground` too) is laid over the muted original, masked to a soft 72px window:
  `mask: linear-gradient(to right, transparent 0, rgb(0 0 0/12%) .675rem, 55% 1.575rem, black 2.25rem, 55% 2.925rem, 12% 3.825rem, transparent 4.5rem, transparent 100%)`.
  The outer element (`live-activity-focus`) is `left:-4.5rem; width: calc(100% + 9rem)` and translates `0 → 100%`. The inner element (`-counter`) translates `0 → -100%`, so the text stays still while the window moves. Both run `2.2s steps(30) infinite`. Under reduced motion or forced colours: `animation:none; opacity:0`.
- **Gating** (lib/visibleAnimation.ts): every animated row sets `--visible-animation-state` to `running` only when an IntersectionObserver sees it, the document is visible, and reduced motion is off. Otherwise it is `paused`, and `will-change: transform` is set only while it runs.
- Streaming answer blocks fade in: `.chat-markdown[data-streaming] > * { transition: opacity 600ms ease-out; @starting-style { opacity: 0 } }` (index.css:2200-2212). This is opacity only, one shot, and never replays on a finished thread.

## 10. Context compaction divider

`ContextCompactionTimelineRow` (MessagesTimeline.tsx:1514-1565):
```
<div role="separator" className="mx-auto flex w-full max-w-(--chat-content-max-width) items-center gap-3 py-1 text-xs text-muted-foreground">
  <span className="h-px flex-1 bg-border/70"/>
  <span className="relative min-w-0 overflow-hidden">
    <Minimize2Icon size-3/> Context compacted 367K <ArrowRightIcon size-3/> 27.7K tokens
  </span>
  <span className="h-px flex-1 bg-border/70"/>
```
- `flex min-w-0 items-center gap-1.5` inside. **The arrow is a 12px ArrowRight icon, not a "→" glyph.**
- Token format `formatTokens` (packages/shared/src/usageFormat.ts:66-79): K/M/B/T with 2 decimals below 10, 1 below 100, 0 at 100 and up, trailing `.0` stripped (`367K`, `27.7K`, `1.25M`).
- A failure gives `CircleAlertIcon`, `text-destructive`, lines `bg-destructive/30`, and ` · reason` at `opacity-80`.
- Titles can replace "Context compacted" (`Context compacted after 3h idle`, idle reasons).
- While running it shows the label with **ActivityShimmerOverlay**.
- The generic `TimelineSystemDivider` (TimelineSystemDivider.tsx) is a different, smaller divider, used for "Subagent of …" and "Context from …": `py-2 gap-2 text-2xs`, with `h-px flex-1 bg-border/70` lines either side, a `font-medium` label, and the detail at `opacity-70` behind `· ` with `max-w-80 truncate`. When it has an action it is a pill: `rounded-full border border-border/70 bg-background px-2.5 py-1 hover:bg-accent hover:text-foreground`.
- "Load earlier turns" uses the same line/label/line pattern: `gap-3 py-1.5 text-xs`, hover foreground.

## 11. User message bubble

`UserMessageRow` (TimelineUserMessageRow.tsx:827-1023):
- Column: `group @container flex flex-col items-end gap-1` (right-aligned, 4px between the bubble and the actions).
- Bubble: `relative min-w-0 max-w-[80%] rounded-2xl text-message-foreground bg-message p-3`. That is **max 80%, 16px radius, 12px padding all round**, on `--message-surface` (dark about `#1e1e1e` on the `#000` ground).
- Consecutive own messages: the joined corners tighten. The one above has `rounded-br-md`, the one below `rounded-tr-md` (6px), and the spacing between them is `pb-1`.
- Text: ChatMarkdown `text-foreground`, `lineBreaks`, raw HTML off.
- **Long messages** (over 600 visible chars or over 8 lines, MessagesTimeline.logic.ts:2648-2672) collapse to `max-h-44` (176px) under a mask `linear-gradient(to bottom, black calc(100% - 1.75rem), transparent)`. A `Button size=xs variant=ghost-muted` reading `Show full message` / `Show less` sits `mt-1.5`, right-aligned.
- **Actions row** (776-822, 1027-1036): `flex w-full max-w-[80%] justify-end pe-1`, then `flex items-center justify-end gap-x-2`:
  - the timestamp `<p class="text-muted-foreground text-xs tabular-nums">8:26 pm</p>` (tooltip `8:26 pm, 9th November 2026`)
  - a `gap-0.5` group of buttons: `Edit from here` (PencilIcon size-3, ghost xs), `Copy message` (MorphIcon Copy→Check size-3; while copied it is disabled, the icon is primary, and an anchored toast shows), `Copy prompt link` (Link icon).
  - Buttons: `size="xs"` = `h-6 px-[7px] text-xs`, `variant ghost-muted` = `text-muted-foreground`, hover `bg-accent text-foreground`.
  - Reveal: **`opacity-0 transition-opacity duration-200`, `group-hover:opacity-100`**, `focus-within:opacity-100`, always shown on coarse pointers. The screenshot's visible time, copy and link icons are this row while hovered.
  - When grouped with the next message on a wide lane (`@min-[37rem]`), the actions float **left of the bubble**, vertically centred.
- Steer marker above: `Redo2Icon size-3 Steer` in `text-xs muted`. `Sent by T3 Code` in `text-2xs muted/70`.

## 12. Assistant message

`AssistantTimelineRow` (TimelineAssistantRows.tsx:27-82):
- `relative min-w-0 px-1 py-0.5`, ChatMarkdown root: `text-sm leading-relaxed text-foreground/[calc(90%+boost/10)]`. That is **14px on 22.75px at 90% foreground**, with `overflow-wrap:anywhere`.
- Markdown (index.css:1870-2140):
  - p/ul/ol/blockquote/pre/table margin `0.65rem 0` (10.4px); the first child has margin-top 0, the last margin-bottom 0.
  - h1 1.25rem, h2 1.125rem, h3 1rem, h4–h6 0.875rem; all `600` weight, line-height 1.3, margin `1.25rem 0 .5rem`; h6 muted.
  - ul `padding-inline-start:1.25rem`, disc.
  - blockquote: 2px start border at currentColor 35%, padding 0.8rem.
  - `strong` 700.
  - inline code: `border 1px transparent; radius .25rem; bg color-mix(fg 10%); padding .1em .35em; font-size .85em; color fg`.
  - pre: `border 1px contrast-border; radius .5rem; bg --muted; padding .8rem .9rem`.
  - Code block shell (MarkdownCodeBlock.tsx:66-90): `my-[0.65rem] rounded-lg border border-border/70 bg-secondary leading-snug dark:border-transparent dark:bg-input/32`. Its header is `pt-1.5 pr-1.5 pl-3`, with a `font-mono text-2xs` file-icon/language label and Copy/Wrap/Run buttons whose icons are `size-3`.
  - links: `--info-foreground`, no underline. Hover shows a dotted underline (`radial-gradient(circle, currentcolor .75px, transparent 1px)` at 4px × 2px, repeat-x).
- **Meta** (shown only on the terminal message once settled; 151-202): `mt-1.5 flex items-center gap-2 text-xs tabular-nums`, `opacity-0 transition-opacity duration-200`, shown on `group-hover/assistant`, focus-within or touch. It holds a status pill when not completed (`rounded-full border px-1.5 py-0.5 font-mono text-3xs`), Copy (ghost xs), Fork (`GitForkIcon size-3`), and the timestamp (`message.updatedAt`).

## 13. Timestamps

- `TimelineRowTimestamp` (TimelineRowParts.tsx:13-42): `pointer-events-none absolute me-1 shrink-0 whitespace-nowrap rounded-md text-muted-foreground text-xs tabular-nums opacity-0`. On `group-hover/timeline-row` or `group-focus-within` it becomes `static opacity-100` (**no transition**: it snaps in and takes layout space only while shown). `alwaysVisible` is used for errors.
- Format `formatDayAwareTimestamp` (timestampFormat.ts:184-200): `Intl.DateTimeFormat(locale, {hour:'numeric', minute:'2-digit'})`, using the 12h, 24h or locale setting (an en-GB locale renders `8:01 pm`). Yesterday reads `yesterday at 8:01 pm`. Older reads `11/9 8:01 pm`, with the year when it is a different year.
- Tooltip `formatChatTimestampTooltip`: `8:01 pm, 9th November 2026`.
- Work rows use the entry `createdAt`, the turn fold the turn start, the assistant meta `updatedAt`, and a user message its `createdAt`.

## 14. Subagent group card ("3 subagents · 1 working · 2 done")

`V2SubagentGroup` (TimelineV2EventRow.tsx:65-163). It forms when **two or more** `subagent` event items are adjacent with the same `runId` and `providerTurnId` (MessagesTimeline.logic.ts:2104-2124).
- Trigger: `flex w-full min-w-0 items-center gap-2.5 py-2 pr-3.25 text-left transition-opacity hover:opacity-100`. It is `text-foreground opacity-100` when expanded or when any agent is active; otherwise `text-muted-foreground opacity-55` (it recedes once all are done).
  - **Avatar stack** `flex -space-x-1.5`: up to 3 `SubagentAvatar`s, each `relative inline-flex size-6 rounded-full border border-border/70 bg-muted ring-2 ring-background` holding a 14px provider icon (or `BotIcon size-3.5 muted`). If there are more, a `+N` chip (`size-6 rounded-full bg-muted text-3xs font-medium ring-2 ring-background`).
  - Text block: `<span class="block text-xs font-semibold">3 subagents</span>` over `<span class="block truncate text-3xs text-muted-foreground">1 working · 2 done</span>`. The status line is `text-info` when active and `text-destructive` when any failed. Order of counts: working, done, failed, stopped, idle, each `N word`, joined by ` · ` (packages/client-runtime/src/state/subagentDisplay.ts:26-41).
  - Elapsed: `text-xs text-muted-foreground tabular-nums`, ticking on `secondTick` (`AgentElapsed`: `45s`, `2m 05s`, `1h 03m`).
  - `ChevronDownIcon size-3.5 muted transition-transform`, `rotate-180` when open (150ms).
- Panel: `CollapsiblePanel animate={false}`, so **no animation**. `mt-1 mb-1 rounded-lg border border-border/60 bg-card/30 p-1`.
- Agent row (`SubagentTimelineLink`, V2LifecycleRow.tsx:397-545): `group/subagent flex w-full items-center gap-2.5 rounded-md px-2 py-1.5`, a button with `hover:bg-accent/20`.
  - The avatar carries a **status dot**: `absolute size-2 rounded-full ring-2 ring-background -right-px -bottom-px`. Colours: `bg-info` for queued/running/waiting, `bg-success` completed, `bg-destructive` failed, `bg-muted-foreground/50` idle, `/60` stopped.
  - Title `text-xs font-medium text-foreground` truncated (plus `on <env>`).
  - Detail line `text-2xs leading-relaxed text-muted-foreground`: progress while running, result when settled, or the status label (`Running`, `Completed`, `Idle · resumable`, `Stopped`). Paths are middle-truncated.
  - Status label at the right `text-xs` when the detail is shown and the agent is not completed.
  - Elapsed `text-xs tabular-nums`, then `ChevronRightIcon size-3.5 text-muted-foreground/60` going to foreground on hover.
  - Tooltip hover card after a 200ms delay.
- A single subagent renders as one lifecycle row with its own status dot.

## 15. Changed-files card ("10 changed files +110 −19")

`ChangedFilesCard` (ChangedFilesTree.tsx:28-147). It sits under the terminal assistant message (`AssistantChangedFilesSection`).
- Card: `@container mt-4 rounded-lg bg-secondary dark:bg-input/20`. There is no border; in dark the fill is about white 2%.
- Sticky header: `sticky top-2 z-10 flex items-center justify-between gap-2 rounded-t-lg bg-secondary px-3 py-2`.
  - Left: `text-xs font-medium`: `10 changed files` plus `DiffStatLabel inline` (`+110` `text-diff-addition-foreground` = emerald-400, `-19` `text-diff-deletion-foreground` = red-400, font-mono, `gap-1`). Counts above 999 compact to `1.2k`.
  - Right: an expand/collapse-all-folders icon button (`ChevronsUpDown` ↔ `ChevronsDownUp` morph, size-3, ghost-muted icon-xs) and `Open diff` (ghost-muted xs, `FileDiffIcon size-3`; the label is hidden below a 24rem container).
- Tree `p-2`. Rows are `h-7` (28px) buttons, `gap-2 rounded-md py-1.5 pr-2 hover:bg-accent/60`, indented `8 + depth*14` px.
  - Directory: `ChevronRightIcon size-3.5 muted/70`, `rotate-90` when open; a `Folder`/`FolderClosed` morph `size-3.5 muted/75`; the name `font-mono text-2xs text-muted-foreground/90`, foreground/90 on hover; a stat `ml-auto font-mono text-3xs`.
  - File: a 14px spacer when there are directories; a Pierre file-type icon `size-3.5 muted/70`; the name `font-mono text-xs text-foreground/85` middle-truncated; the stat right-aligned.
  - Folders start collapsed (persisted per run). The tree virtualises past a row threshold. Remaining files show as `N more files in the full diff` (ghost-muted xs).
- Clicking a file opens the diff at that file. Right-click gives a file context menu.

## 16. "Waiting on 2 subagents … Details Stop" (ThreadNow line) and the Background work dialog

- This is the timeline **footer** (ChatView.tsx:3081+), the last thing in the list above the composer, not a banner. `ThreadNowLine` (ThreadNow.tsx:18-66):
  `mx-auto flex w-full min-w-0 max-w-(--chat-content-max-width) items-baseline gap-1.5 px-1 pb-2 text-xs text-muted-foreground [&_svg]:size-3.5`. It fades in with `transition-opacity duration-150 ease-out starting:opacity-0`.
- Icon: `TerminalIcon` if every task is a command, otherwise `CircleDashedIcon`.
- Title (`presentPendingBackgroundWork`, packages/client-runtime/src/state/threadExecution.ts:568-616): `Waiting on 2 subagents` or `Running 1 command and 2 subagents` (joinWithAnd of kind counts). A single named task reads `Waiting · <label>`, an unnamed one `Waiting on a subagent`.
- Description after it at `text-muted-foreground/60`: the newest progress (`label: activity`), or the labels joined by `, `.
- Actions `ms-1.5 flex shrink-0 items-baseline gap-2.5`: a ticking `formatDuration` when there is a single task; **`Details`** (InlineButton: `font-medium text-foreground hover:underline underline-offset-2`); `Open` (muted) for a single child thread; **`Stop`** (InlineButton, `Stopping...` while it stops).
- `Details` opens a **Dialog titled "Background work"** (BackgroundWorkDetails.tsx). It is `space-y-5`, one section per task (`space-y-2 text-sm`): `h3.font-medium` title with `Waiting`/`Running` (`text-xs muted`) at the right; the command in `rounded-md border border-border bg-muted/20 p-3 font-mono text-xs whitespace-pre-wrap`; live output (CommandOutputPanel, max-h-80); and an `Open thread` inline button.
- Other ThreadNow lines share the same look: stopping (`Stopping` / `Agent not responding to Stop` + `Force stop`), the goal, sync status, and child input requests.

## 17. Inline thread links and file links

- `MarkdownThreadLink` → `ThreadContextChip` (components/ThreadContextChip.tsx). Markup: `ContextChip kind="thread"` rendered as a router `<Link>`:
  `inline-flex max-w-full items-center gap-[0.2em] font-medium leading-[normal] rounded-sm [&_svg]:size-[1em]` (components/ContextChip.tsx:9).
  It contains the **project favicon at 1em** (or `MessagesSquareIcon`) and the thread title, in the inherited text colour at medium weight, with no underline. Tooltip `Open thread`. An unresolved link becomes an `UnresolvedChip`.
- File links (`MarkdownFileLink.tsx:320-372`) are the same chip with a file-type icon (`FileTagChipContent`). Click opens a preview, ⌘/ctrl-click opens it in the editor, and right-click gives a menu.

## 18. "Scroll to end" pill

`ScrollToEndButton` (TimelineOverlays.tsx:26-54):
- `chat-scroll-to-bottom pointer-events-none absolute z-30 flex justify-center py-1.5`, `bottom = composer clearance + 4px`, centred on the lane.
- A `Button size="xs" variant="glass"`: `surface-glass` = `color-mix(background 80%, transparent)` with `backdrop-filter: blur(12px | dark 16px) saturate(1.14 | 1.08)`, `rounded-full border-border/60 shadow-sm`.
- Content: `ChevronDownIcon size-3.5` + `Scroll to end` + an optional `<span class="text-muted-foreground">· {summary}</span>`. The summary is the child-input waiting label and/or `N queued`, joined by ` · ` (ChatView.tsx:3075-3078).
- Pointer-down is prevented so the composer keeps focus.

## 19. Cite bubble on selection

`AssistantSelectionToolbar` (AssistantSelectionToolbar.tsx; lib/selectionActions.ts):
- Shown when a selection lies inside an assistant message (`data-assistant-citation-source`) and is visible in the viewport.
- It is placed at the **mouse-up pointer position** (`position: fixed; left/top = clientX/Y`), clamped 8px inside the window. Without a pointer it goes at the selection rect's right edge, `bottom + 4px`.
- It appears immediately, or after 500ms following a double or triple click. Scrolling settles for 150ms.
- A `Button size="xs" variant="glass"` (same glass as Scroll to end) with `QuoteIcon size-3.5` and `Cite`. When the selection is too long it reads `Shorten selection` (disabled).
- Tab moves focus to it, Escape dismisses it, and pointer-down is prevented so the selection survives.

## 20. Minimap

TimelineMinimap.tsx: a `w-18` (72px) strip at the left edge, `z-40`. It is hidden until hover on fine pointers (`opacity` 150ms) unless there is a persistent gutter (48px).
- Ticks: `h-0.5 w-6 rounded-full`, `bg-muted-foreground/25`, the current one `/60`, with an in-view overlay `after:bg-foreground/55` fading over 150ms.
- A vertical guide `left-3 w-px bg-border/15`.
- Hover preview: `dropdown-glass rounded-xl p-3 shadow-xl w-80`, title `text-sm font-medium`, body `text-sm muted` clamped to 3 lines.
- Spacing 8px per item, max height `calc(100vh - 18rem)`. Selecting a tick scrolls to it with `viewOffset: 24`, animated unless reduced motion is on.

## 21. Motion summary (T3)

| What | Spec |
| --- | --- |
| Work-entry chevron | rotate 90°, `transition-transform duration-200` (200ms, `cubic-bezier(.4,0,.2,1)`) |
| Turn-fold chevron, copy→check, folder icons | `morphicons` glyph morph |
| Subagent group chevron | rotate 180°, default 150ms |
| Disclosure bodies (groups, folds, subagent panel, entry details) | **no height or opacity animation**: rows insert or remove instantly |
| Hover colour | `transition-colors` 150ms on fold labels and buttons |
| Message actions, assistant meta | opacity 0→1, 200ms |
| Row timestamp | instant (no transition) |
| Shimmers | 2.2s `steps(30)` infinite, 72px band; paused offscreen |
| Streamed markdown blocks | opacity 600ms ease-out via `@starting-style` |
| ThreadNow line | opacity 150ms ease-out on entry |
| Live follow | LegendList `maintainScrollAtEnd animated:true` while working |

---

# Ferrite comparison: concrete gaps

Ferrite files read: `crates/ferrite-pane/src/transcript.rs`, `transcript/rows.rs`, `pane.rs`, `pane/text.rs`, `pane/working.rs`, `pane/agents.rs`, `crates/ferrite-ui/src/theme.rs`, `motion.rs` (catalog), `components.rs` (shimmer), `crates/ferrite-core/src/progress.rs`, `clock.rs`, ADR 0006/0007.

ADR 0007 deliberately makes Claude the presentation reference ("no icon, no time" activity, ∴ reasoning, a `Worked for` stamp at the end of the turn). Matching T3 means **superseding ADR 0007** with a new ADR. Most of the gaps below follow from that ADR.

## Structural gaps (largest first)

1. **No turn fold.** In T3, a finished turn's work collapses (by default) into one `Worked for 24m 55s ›` row at the **top** of the turn, with a hairline under it and a hover timestamp, leaving only the final answer visible. Ferrite leaves every group and call in place and closes the turn with a stamp at the **end** (`pane.rs:3892 turn_end`: words plus a `LINE` running to the column's end). It shows that stamp only when the turn took 30s or more (`theme.rs:1621 TURN_STAMP_MIN_MS`); T3 has no minimum. Needs: a new `RowId::TurnFold`/`RowKind` in `transcript/rows.rs` (`project`), turn-scoped disclosure state, hiding rules (keep the terminal answer, compactions, live agents, notices, Q&A, failures), and steer segments (`Continued for …`).
2. **Working header placement.** T3: `Working for 11m 24s` sits under the prompt at the top of the turn (`border-b` hairline at 60%, h-6, text-sm muted, no shimmer), and a separate live row at the tail shows the current tool (`Running sed`, shine) or `Thinking` (shimmer), with a thought line above it. Ferrite: one combined working line at the tail (`pane/working.rs:46 working_tail`, `RowId::Working`): spinner mark, shimmering caption, `1m04s · ↓ tokens · thinking with … effort`. Ferrite also hides the running call's own row (`rows.rs carried_tool`), which T3 does not.
3. **Icons per row.** T3: a 24px gutter holding a 16px Lucide icon (terminal, brain, square-pen, eye, search, globe, hammer, wrench, bot, zap, message-circle, list-todo) at muted 70% on every work row and group header. Ferrite: "no icon, no time" (`theme.rs` transcript grammar, about line 1382), with `∴` for reasoning only (`pane.rs:3081`). This needs an icon on `tools::policy` (`crates/ferrite-core/src/transcript/tools.rs`) and an icon gutter in `pane.rs:3988 activity_line`. Ferrite's `ICON_STROKE` is 1px; Lucide's is about 1.33px at 16px.
4. **Expanded group container.** T3: no card. A plain list of 24px rows (`py-0`, no separators), capped at `min(18rem, 50dvh)` plus opened children, scrolling inside, with 24px top and bottom **fade masks** only when it overflows, follow-on-append, and a remembered scroll position. Ferrite (`pane.rs:4345 render_tool_group`): a `CARD_RING` card at `R_CARD` 10px, `TOOL_CARD_ROW_H` 36px rows with hairlines between them, `CARD_OUTSET`, unbounded height, and no fades.
5. **Chevron placement.** T3: on **entry rows**, at the far right (12px, muted 70%, slot always reserved, rotate 90° over 200ms). **Group headers have none** (icon + summary + hover time). The turn fold's 14px chevron sits right after its label and morphs. Ferrite: the chevron sits right after the summary text, 12px `TEXT_FAINT`, turning over 60ms (`pane.rs:4563 disclosure_chevron`, `motion::CHEVRON`). Groups always show it; other rows show it only on hover.
6. **Hover timestamps.** T3 shows a 12px muted `8:01 pm` on hover for every work row, group header and turn fold. Ferrite has no time on activity rows.
7. **Expanded thought.** T3: the label becomes `Thought` (or `Thinking`), and the body is indented 28px (`ms-7`), **full foreground** markdown, `gap-3` paragraphs, up to 384px with its own scroll, following the live text. Ferrite (`pane.rs:3079-3127`): `∴` mark, the header keeps its summary sentence, and the continuation and rest are `INK_SUBTLE` with no cap. Ferrite also shows short thoughts whole with no disclosure; T3 always shows one line.
8. **Single tool calls aren't grouped in T3.** One tool is one entry row with its own label. Ferrite groups only at two or more too (`rows.rs group_at`), so this matches, but Ferrite's single row is the `Ran \`cmd\` · result` call line (`pane/text.rs CallLine`). T3 shows the plain label and puts results only in the expanded panel.
9. **Summary grammar.** T3: at most **2** kinds by priority (commands and edits first), joined by ` and ` (`Ran 4 commands and changed 10 files`), Oxford comma for 3+, the rest as `performed N other actions`, edits as `Changed N files` (unique files), always past tense, failures never in the text. Ferrite (`pane/text.rs:448-580`): comma-joined (`Ran 2 commands, read 1 file`), up to `SUMMARY_KINDS`, `and N more`, present tense while running (`Running 2 commands`), `Edited`/`Wrote` naming a single file plus `+N −N` marks, and `(1 failed, 1 stopped)` suffixes.
10. **Live shimmer spec.** T3: two effects at **2.2s `steps(30)`**. One is a 72px band clipped to the text (`live-tool-shine`) on running tool labels; the other is a 72px masked overlay (12/55/100/55/12% stops) on Thinking and compaction. Both run on any visible live row, not only the focused pane. Ferrite (`components.rs:528-600`, `theme.rs:3203`): **2.4s continuous** frame loop, a crest of ±0.45 of the run width (it scales with text length, unlike T3's fixed 72px), lifted 75% toward white or the clay `#ffe1d3`, only on the focused Pane's working caption.
11. **Compaction divider missing.** T3: a centred `Minimize2 · Context compacted 367K → 27.7K tokens` divider (text-xs, `gap-3`, `bg-border/70` lines), shimmering while active, with failure and idle variants. Ferrite only maps `compact_boundary` to a working-caption phase (`crates/ferrite-core/src/providers/claude/wire.rs:442`). It has no row and no token counts. `formatTokens` rules are in §10.
12. **Subagent card.** T3: `3 subagents` (xs semibold) over `1 working · 2 done` (3xs, info/destructive), a stacked 24px avatar trio with ring-2, a ticking elapsed time, ChevronDown rotate-180, dimming to opacity 55% once settled, and a bordered `bg-card/30` panel of agent rows with 8px status dots, progress/result lines and per-agent elapsed. Ferrite (`pane/agents.rs`): Claude's `Ran 2 agents` group with `Ran agent <task>` rows. There are no avatars, no status-count line and no per-agent durations.
13. **Changed-files card.** T3: under the final answer, `10 changed files +110 -19` with a sticky header, an expand-all-folders control, `Open diff`, and a collapsible 28px-row folder tree with per-node stats. Ferrite: the turn's output cards (`theme.rs:1650 OUTPUT_CARD_*`, `rows.rs RowId::Outputs`), one 400×56 card per written page or file. It has no aggregate diff tree; the tree exists only in the Review (`crates/ferrite-cockpit/src/review.rs`).
14. **Background-work line above the composer.** T3: a ThreadNow footer line `Waiting on 2 subagents · <activity>  Details  Stop`, with a `Background work` dialog. Ferrite explicitly has "no shelf" (`pane/composer.rs:365`); its background work lives in the rail and the Background tasks pane (`pane/working.rs:192 background_clock`).
15. **Scroll to end.** T3: a glass pill (80% ground, backdrop blur 16px, rounded-full, h-6, text-xs) with `⌄ Scroll to end · 1 queued`. Ferrite: a 32px flat circle with an arrow and a `Jump to latest` tooltip (`theme.rs JUMP_LATEST`, `transcript.rs:1963`).
16. **Selection action.** T3: one `❝ Cite` glass button at the pointer. Ferrite: a `Start a side chat | Reply` pill centred over the selection's first line (`theme.rs:1692-1705`).
17. **Thread link chips.** T3 renders thread links as inline medium-weight chips with a 1em project favicon and the thread title. Ferrite has file links (`crates/ferrite-pane/src/file_links.rs`) but no thread-link chip.

## Metric and visual gaps

| Item | T3 | Ferrite | Ferrite location |
| --- | --- | --- | --- |
| Ground | `#000` | `#1a1a1a` (`CARD`) | theme.rs:239 |
| Column max | 736 (default) / 1152 (wide) / 100% | 736 fixed | theme.rs:1514 `TX_COLUMN_MAX_W` |
| Side padding | 48px (≥640px), 12px narrow | 28 Solo / 18 Group | theme.rs:1508 |
| Prose | 14 / 22.75 at foreground 90% | 14 / 21 `INK_STRONG` | theme.rs:750-753, 826 |
| Work row | 14px on a 24px min row, muted `#a1a1a1`, consecutive rows 0 gap | block-size 13px (`Grid::activity`) on a 21 line, `INK_SUBTLE #9c9c9c`, half-line gaps between calls | pane.rs:2912, rows.rs gap_before |
| Paragraph gap | 10.4px | 12px | theme.rs:1497 |
| Prompt bubble | max 80%, radius 16, padding 12, about `#1e1e1e` | max 84%, radius 10, padding 16×10, `#262626` | theme.rs:1536-1539, 273 |
| Prompt time | absolute `8:26 pm` / `yesterday at …` | relative `5 minutes ago` | crates/ferrite-core/src/clock.rs:118 |
| Message actions | xs ghost buttons h-6 with 12px icons; opacity 200ms; time, edit, copy, link | 24px squares with 14px glyphs; 120ms after a 100ms delay; time and copy | theme.rs:1634, motion catalog |
| Long prompt | clamps at 600 chars or 8 lines to 176px under a 28px fade, with `Show full message` | none seen | — |
| Live timer | `11m 24s` (spaced, zero parts dropped) | `11m24s` (`live_seconds`) | progress.rs:514 |
| Settled duration | `5m` (zero seconds dropped), `4.1s` below 10s | `5m 0s`, whole seconds | progress.rs:529 |
| Turn duration shown | always | at 30s or more only | theme.rs:1621 |
| Disclosure animation | none (snap) | none (snap) | matches |
| Chevron turn | 200ms | 60ms | motion.rs:303 |
| Stream fade | per block, 600ms ease-out | per row or run, 400ms linear | theme.rs:3076 |
| Failed tool | icon tinted `#fca5a5`/40 plus a 12px X; no text | red `Failed` word in the verdict | pane.rs:4199-4220 |
| Diff minus | ASCII `-` | `−` (U+2212) | text.rs:571 |

## Suggested order
1. A new ADR superseding 0007's "no icon, no time / stamp at the end".
2. The turn fold plus the top `Working for` header (gaps 1–2), with the timer format (`progress.rs`).
3. Icon gutter, hover timestamp and right-edge entry chevrons (gaps 3, 5, 6).
4. A capped expanded group list with fade masks, without the card (gap 4).
5. Summary grammar (gap 9), the thought expansion (gap 7), and the 2.2s stepped shimmers (gap 10).
6. The compaction divider, subagent card, changed-files card, background line, scroll pill and Cite button (gaps 11–17).
