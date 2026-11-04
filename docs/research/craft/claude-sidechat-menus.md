# Claude Desktop: side chat, right-click menus, subagent drill-in

Captured from the live Claude Desktop app (Code tab) on 2026-11-04. The
captures are in `claude/`.

## Side chat (`sidechat-open.png`, `sidechat-expanded.png`, `sidechat-motion.mp4`)

- **Opening.** Select text in an answer. One dark rounded pill appears above the selection (`claude/selection-pill.png`): `Start a side chat` | `Reply`.
  - A thin vertical divider separates the two.
  - There is no ↩ glyph.
  - The selection itself is opaque `#314B69` (`claude/selection.png`).
  - Ferrite's pill today has no divider and shows `Reply ↩`.
- The owner is happy with Ferrite's own selection colour; it stays. Drag-select without a double-click is broken in Ferrite and is being fixed.
- **The panel.** `Start a side chat` opens a **right-docked pane** at about 420 pt beside the thread. The thread column reflows narrower; it is not a floating card over the thread.
- **Pane head.** A tab strip with `Side chat ×`, then `+`. At the right: expand (⤢) and close (×).
- **Body.** Empty, it reads `No messages yet`, centred, in muted ink.
- **Composer.** At the pane's foot, pre-filled with the selection as a quote: `> …selected text…`. When empty, the placeholder is `Ask a quick question`, with ↵ at the bottom right.
- **More tabs.** `+` opens a new tab, `Side chat 2`. Each tab is its own quick conversation about this thread.
- **Expanded.** Expand (⤢) makes the pane fill the whole content area, so the thread is hidden. The same control collapses it.
- **Motion.** Open, expand, collapse and close are all **instant** (one frame at 120 fps). Nothing slides or fades.

## Right-click menus (`rc-answer-text.png`, `rc-prompt.png`)

- **On an answer,** anywhere in it, including over a selection, which the right-click clears:
  - `Copy message`
  - `Copy message as Markdown`
  - ─
  - `Attach message as context`
  - `Pin as chapter`
  - ─
  - `Fork from here`, with the subtitle `Starts a new session, keeps this one`
- **On a user prompt:**
  - `Copy message`
  - `Copy message as Markdown`
  - ─
  - `Attach message as context`
  - ─
  - `Rewind to here`, with the subtitle `Takes this and what follows out of the conversation`
  - `Fork from here`, with the subtitle `Starts a new session, keeps this one`
- **Style.** All rows are the same size, and there are no shortcut glyphs. Subtitles are a smaller line in muted ink under the label. The menu appears in one frame.
- **Not captured.** Right-clicking a link (the thread had no links). It needs a capture before being matched.

## Answer actions on hover

There are four icons under an answer, then the time:
1. `Copy`
2. `Fork from here`: a two-line tooltip, "Starts a new session from this point. This one stays as it is."
3. `Pin as chapter`
4. `Read aloud` (owner: not wanted in Ferrite)

## Subagent drill-in (`agents-group-open.png`, `subagent-drillin.png`)

- **The group.** `Ran 2 agents ▸` expands to rows: `Ran agent` in muted ink, then the agent's description, inside a faint rounded box.
- **Opening one.** Clicking a row underlines it and opens the **same right-docked pane**. The head reads `‹ Count .rs files in ferrite-ui`, with expand and close.
- **Pane body:**
  1. the agent's prompt, as a user bubble;
  2. `Model Haiku 5.5` in muted ink;
  3. the agent's collapsed tool summary (`Counted .rs files recursively under ferrite-ui ▸`);
  4. its answer.
- **Motion.** Instant, like the side chat.

## Reply: a floating popover (`claude/reply-popover.png`)

Pressing `Reply` in the selection pill opens a **floating card anchored just above the selection**, over the transcript. It does not go to the main composer.

- **Quote.** The top of the card shows the selected text as a quote: two lines at most in muted ink, ending in "…".
- **Input.** A single-line input under the quote, with the placeholder `Reply to Claude`.
- **Send.** A small icon button at the right of the input.
- **Card.** A dark raised card with rounded corners, a hairline border and a soft shadow. It sits about 280 pt wide, while the selection stays highlighted.
- **What sending does** (`claude/reply-sent-chip.png`): it does not send a message. The reply becomes an **attachment card in the main composer's tray**, and the popover closes.
  - The card is about 120×120 pt, rounded, on a darker ground.
  - Its top shows a quote bar with the typed reply ("testing").
  - Its foot carries a small outlined `Reply` badge.
  - The user then writes the main message and sends both together. Several replies can stack as several cards.

This is the "floating panel" the owner wants: Ferrite's Reply must become this popover. Ferrite currently puts the quote in the main composer.

## Wrap-up: the end-of-turn box (`claude/parity/wrapup-*.png`)

Captured 2026-11-04 from Claude Desktop 's Code tab (a scratch repo, Opus 5.5), the agent calling `wrap_up`. Measured in points at 1x from `wrapup-question.png` and `wrapup-used.png`.

**Live (`wrapup-question.png`, `wrapup-hover-*.png`).** It sits right under the final answer, on the content column, 24 pt below the answer's last line.

- **Ground and border.** No ground of its own (the page's #181818 shows through). A 1 pt ring, #212121 (the card ring), at about a 10 pt radius. It is full column width (735 pt in a 1920 pt window).
- **Padding and rhythm.** 16 pt on all sides, and 16 pt between the parts, in one column: recap, question, buttons. The box is 132 pt tall for one line of each.
- **Type.** All at the answer's 14 pt on a 20 pt line, regular weight. The recap is muted (#9b9b9b). The question is the strong ink (#efefef), not bold.
- **Buttons.** One row, 12 pt apart, each 28 pt tall at a 6 pt radius, 12 pt side padding, the label at the body size.
  - The first (recommended) is primary: a #ededed fill with dark ink, and #d1d1d1 under the pointer. It carries `⌘` `↩` keycaps after its label.
  - The second is secondary: a #3a3a3a fill with the strong ink. This is the colour of an approval's `Deny` (`SECONDARY`).
  - At most two. The label is what is sent.
- **Hover.** Only the fill steps (primary to #d1d1d1); nothing moves, no ring.

**Pressing a button (`wrapup-used.png`).** The label is sent at once as an ordinary prompt: a right-aligned bubble, and the turn runs. The box does not vanish. It is spent:

- it keeps its ring and its words, but its width shrinks to its words, wrapping at about 480 pt;
- both buttons are gone;
- the step that was sent stands as a muted line (#9b9b9b) 8 pt under the question.
- A box left behind by a later prompt reads the same, so no box from an earlier turn offers anything.

**Recap only (`wrapup-recap-only.png`).** No box is drawn. The recap is used as the session's status, and the agent's own prose is all the transcript shows. §13's rule holds.

**Not captured.** The `blocked` state: the agent did not send one, so how Claude draws a box that waits with no question or step is unseen. Ferrite draws the box when `blocked` is set, as the live box with no buttons. Keycaps are not drawn: no chord is bound to the first step.
