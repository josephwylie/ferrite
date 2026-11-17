# Panel runtime correction: ordinary use permanently enlarges the panel

This corrects the evidence behind `panel-tools-018`; it is not a new feature inventory or a new issue count. The sticky-width portion is **runtime reproduced**. The narrow-window portion remains unverified. `motion-001` remains source-proven by this headless investigation; settled screenshots cannot establish transition timing.

## The defect the operator experiences

On a fresh thread, Overview is 320 logical pixels wide. Opening Files expands the panel to 864 px, automatically hides the 280 px navigation sidebar, and leaves only 576 px for the conversation. Switching back to Overview leaves it at 864 px. Closing Files, closing/reopening the panel, and even restarting the app do not recover the original compact geometry. The operator never dragged a divider.

This is a 2.7× enlargement: 22.22% to 60% of the 1440 px window. Conversation width falls from 840 px to 576 px when returning to the exact same Overview content. The compact state has become a persistent large state through an ordinary tab visit. An empty panel also keeps the 60% allocation.

A second fresh thread reproduced the same defect using Changes instead of Files. Its executable check failed on the actual window measurements: `Overview width changed without a resize: 320.0 -> 864.0`.

The useful contract is relative to the observed initial geometry: returning to Overview without an operator resize should recover Overview's prior compact width. This does **not** claim that 864 px is universally wrong for Files, Browser, or a deliberately expanded panel, or that every first opening is oversized.

## Runtime and evidence

- Existing production GPUI Cockpit, `target/debug/ferrite --headless`, source commit `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- Binary SHA-256 `8f47873505f48c90a267ef5b02913a345b72e390d17b78cd3a72c552497a2b98`; no rebuild or application code edits.
- Fresh isolated Store `/tmp/ferrite-audit-runtime-9y65u3_6/threads`; two fixture threads, fake local provider version `9999.0.0`; no real model prompt.
- Actual window size 1440×900 logical pixels. Screenshots 2880×1800 pixels at scale 2. Measurements below use logical pixels, not screenshot pixels.
- Only public `window key`, `window click`, `window find`, and `window shot` commands drove the app. The driver logs are [commands.jsonl](../evidence/panel-correction/commands.jsonl); raw element rectangles are [measurements.jsonl](../evidence/panel-correction/measurements.jsonl).

| Actual state | Left nav | Right panel | Panel/window | Conversation board | Screenshot |
|---|---:|---:|---:|---:|---|
| Fresh Overview reopened before any tool | 280 | 320 | 22.22% | 840 | [03](../evidence/panel-correction/03-overview-reopened.png) |
| Files opened | hidden | 864 | 60% | 576 | [04](../evidence/panel-correction/04-files-open.png) |
| Back to Overview | hidden | 864 | 60% | 576 | [05](../evidence/panel-correction/05-overview-after-files.png) |
| Panel closed | 280 | closed | 0% | 1160 | [06](../evidence/panel-correction/06-closed-after-files.png) |
| Overview reopened | hidden | 864 | 60% | 576 | [07](../evidence/panel-correction/07-reopened-after-files.png) |
| Files tab closed, only Overview remains | hidden | 864 | 60% | 576 | [08](../evidence/panel-correction/08-overview-files-tab-closed.png) |
| Overview explicitly maximized | hidden | 960 | 66.67% | 480 | [09](../evidence/panel-correction/09-overview-maximized.png) |
| Overview restored | hidden | 864 | 60% | 576 | [10](../evidence/panel-correction/10-overview-restored.png) |
| Left nav forced open alongside large Overview | 280 | 680 | 47.22% | 480 | [11](../evidence/panel-correction/11-nav-toggle-large-panel.png) |
| All tabs closed, empty panel | hidden | 864 | 60% | 576 | [12](../evidence/panel-correction/12-empty-after-files.png) |
| Browser opened after Files | hidden | 864 | 60% | 576 | [13](../evidence/panel-correction/13-browser-after-files.png) |
| Changes opened after Browser | hidden | 864 | 60% | 576 | [14](../evidence/panel-correction/14-changes-open.png) |
| Back to Browser | hidden | 864 | 60% | 576 | [15](../evidence/panel-correction/15-browser-after-changes.png) |
| Browser after full headless app restart | hidden | 864 | 60% | 576 | [21](../evidence/panel-correction/21-after-headless-restart.png) |
| Independent fresh thread: Overview | 280 | 320 | 22.22% | 840 | [22](../evidence/panel-correction/22-independent-overview-before-changes.png) |
| Independent fresh thread: Changes | hidden | 864 | 60% | 576 | [23](../evidence/panel-correction/23-independent-changes-open.png) |
| Independent: Overview returned and reopened | hidden | 864 | 60% | 576 | [25](../evidence/panel-correction/25-independent-overview-reopened.png) |

The actual Browser body cannot run in this headless build and displays its browser-unavailable explanation. That is an environment constraint, **not a product browser failure**. The geometry and production browser chrome in those captures are real. The fixture working directory deliberately is not a Git repository; the Changes message reflects that fixture, not a broken Git integration.

## Reproduction and the failed check

Starting with a fresh focused fixture thread and the panel closed:

1. `window key cmd-alt-b` opens Overview; `window find panel` reports width 320.
2. `window key cmd-shift-f` opens Files; width becomes 864 and `window find nav-column` reports nothing drawn.
3. `window click --selector panel-tab-Overview`; width remains 864.
4. `window click --selector panel-tab-Files-close`; width remains 864.
5. `window key cmd-alt-b` twice closes and reopens Overview; width remains 864.

The independent second-thread reproduction uses `cmd-shift-d` and is preserved as [reproduce-width.py](../evidence/panel-correction/reproduce-width.py). It creates a fresh thread in the already-running isolated fixture, drives public inputs, measures real rectangles, and asserts the width contract. Actual command:

```sh
python3 docs/audits/2026-11-13-t3-parity/evidence/panel-correction/reproduce-width.py
```

Observed exit 1, with [captured failure output](../evidence/panel-correction/reproduce-width-output.txt) and [structured assertion result](../evidence/panel-correction/independent-width-contract.json). It ran in approximately 9.4 seconds, including four settled screenshots. The fixture is now stopped; restart an isolated fixture before rerunning the command. `drive.py prepare` deliberately refuses to overwrite this evidence's existing state file.

## Root cause, inspected after runtime reproduction

`floor_w` in `crates/ferrite-cockpit/src/cockpit/panel.rs:149` selects a compact 320 px Overview floor, but Files/Changes use 60% of the room right of the nav. `tend_panel` at line 368 permanently replaces `ThreadSurfaces.width` with the active surface's larger floor at lines 381–382, then saves the surfaces at line 393. `panel_w` at line 171 subsequently takes the maximum of this stored width and the active floor. Returning to Overview therefore cannot reduce the width.

This state owns both the operator's width and automatic expansion. The observed nav auto-fold is part of the same interaction: `cockpit.rs:2841` folds navigation when the remaining board becomes too narrow; `floor_w` then sees zero nav width, making the Files/Changes floor 60% of the full window. [saved-panel.json](../evidence/panel-correction/saved-panel.json) contains width `864.00006`, `maximized: false`. The state survives restarting the headless app (screenshots 20 and 21).

Competing explanations checked against the real window:

- **Files still being open imposes the width:** disproved by closing its tab while Overview remains at 864.
- **Panel accidentally maximized:** disproved by distinct 960 px maximize geometry, a restore back to 864, and persisted `maximized: false`.
- **Only Files causes it:** disproved by the independent Changes-only fresh thread.
- **Left sidebar itself gets wider:** not observed. With the right panel closed, repeated left-nav toggles consistently return to 280 px.

## Left sidebar and motion: exact boundaries

The left sidebar was measured separately with the right panel closed. `cmd-b` closes it (board 1440 px); another `cmd-b` restores 280 px (board 1160 px). Repeating close/open restores the same rectangles. Screenshots [17](../evidence/panel-correction/17-nav-closed.png), [18](../evidence/panel-correction/18-nav-reopened.png), and [19](../evidence/panel-correction/19-nav-reopened-again.png) establish those endpoints. They do not contradict the user's report that the transition snaps.

The public headless driver has one fixed size (`headless.rs:188`), no window resize, no divider drag, and no per-frame capture command. Each key advances 60 ms (`headless.rs:297`); each mouse event advances 30 ms (`:337`); `shot` advances six additional 60 ms ticks before capture (`:387`). Screenshots therefore settle for at least 360 ms and cannot prove whether an intermediate animation existed. Actual native recording is needed for visual motion verification. A source-only finding must not be promoted to runtime verified from these PNGs.

Source remains consistent with the reported snap: `nav_width` (`cockpit.rs:2829`) selects zero or the full width; `set_nav_collapsed` (`:6186`) changes a Boolean and notifies; `nav` (`:12845`) removes the column when folded; the panel's element takes `panel_column_w` directly (`panel.rs:615`). No application change was made to probe this.

Narrow-window sizes, manual divider resizing, restored operator-selected widths, mid-transition reversals, and native browser viewport resizing are **not tested here**. Only the fixed 1440×900 runtime geometry and the actions above are promoted. T3 comparison remains in the existing source report; these measurements make no new claim about T3's runtime.

## Repair contract and cleanup

Keep an operator's chosen width separate from the active tool's temporary minimum. Verify Overview → Files/Changes → Overview, closing/reopening, empty tabs, restart, maximize/restore, and nav auto-fold against the same contract. A deliberate wide tool may still have a broad minimum; visiting it must not silently turn that temporary minimum into the saved compact width.

Both fixture threads were explicitly closed; `thread list` returned an empty list. All six tracked PIDs (server, old/restarted headless windows, and fake providers) and all owned process groups were verified absent. [cleanup.json](../evidence/panel-correction/cleanup.json) records the commands and process checks. No builds or Rust tests ran: this is an audit-only evidence correction.
