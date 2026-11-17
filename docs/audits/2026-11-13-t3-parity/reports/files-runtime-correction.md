# Files and Changes runtime correction

This run extends the earlier narrow Files runtime pass. It covers external file changes and revisit, file-tree invalidation, a Changes diff larger than 120 rows, and inner Files tab overflow. It does not duplicate the already documented editor/filter focus-loss result.

## Runtime setup

- Ferrite source commit: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- Existing executable: `target/debug/ferrite`; SHA-256 `8f47873505f48c90a267ef5b02913a345b72e390d17b78cd3a72c552497a2b98`.
- A fresh scratch Git repository and `FERRITE_STORE` were created under `/tmp/ferrite-audit-runtime-hfttwj96`. Settings, provider shims, thread data, and fixture files stayed isolated there.
- Provider calls were directed to the existing fake Claude protocol peer. No real model/provider was used. No application source was edited, built, or tested.
- UI evidence came from Ferrite's production headless driver at its 1440×900 logical window size. Commands and stdout/stderr are in [commands.jsonl](../evidence/files-correction/commands.jsonl); startup logs are in [server.log](../evidence/files-correction/server.log) and [headless.log](../evidence/files-correction/headless.log). Structured cases and cleanup evidence are in [cases.json](../evidence/files-correction/cases.json).

## Results

### Files keeps a stale preview when revisiting an open path

**Status: broken.** I opened `README.md`, replaced its contents externally with `EXTERNAL_REOPEN_SENTINEL`, then selected the already-open file again. The rendered preview still showed its original text. After closing the inner tab and opening the same path from the tree, the preview showed the external replacement. This establishes stale contents on revisit and successful disk reload after close/reopen for this clean Markdown fixture; it does not test dirty-buffer conflict behavior.

Evidence: [original](../evidence/files-correction/07-readme-original.png), [same-tab revisit](../evidence/files-correction/08-readme-revisited-after-external-change.png), [close/reopen](../evidence/files-correction/09-readme-reopened-after-close.png).

### The open file tree does not refresh added or removed children

**Status: broken.** With `tree-fixture` expanded, I added `added-runtime.txt` and removed `preexisting.txt` from the filesystem. The displayed tree continued to show the deleted child and omit the added child. Collapsing and re-expanding the folder did not refresh its cached contents. This case establishes stale directory children; it does not separately establish how Git status colors update after an external status change.

Evidence: [before mutation](../evidence/files-correction/10-tree-expanded-before-mutation.png), [after mutation](../evidence/files-correction/11-tree-after-external-mutation.png), [after collapse and re-expand](../evidence/files-correction/12-tree-after-collapse-expand.png).

### Changes counts the full long diff but the panel hunk renderer truncates it

**Status: partial, with an explicit truncation boundary in source.** The fixture replaced 160 lines, including a unique sentinel on its final line. Changes listed `long-diff.txt` with `+160 −160`; the visible panel began rendering its hunk. The existing renderer defines `MAX_LINES = 120`, emits at most that many rows, then appends an `… +N lines` notice (`crates/ferrite-cockpit/src/cockpit/panel/hunks.rs`). The final sentinel therefore lies outside the panel hunk's 120-row cap even though the diffstat counts all changes.

The headless driver does not expose a scroll operation, and the attempted `pagedown` key was reported unhandled. Therefore this run does not claim runtime evidence for the rendered omission-count line, scrolling to the end of the panel, or the separate full-review route. It establishes the full diffstat plus the first rendered hunk region, and corroborates the known 120-row rendering limit with source. The separate Review changes flow remains unchecked.

Evidence: [long diff in Changes](../evidence/files-correction/14-changes-long.png), [End key attempt](../evidence/files-correction/15-changes-end.png), [unhandled PageDown attempt](../evidence/files-correction/17-changes-pagedown.png).

### Inner Files tabs overflow without their own navigation

**Status: broken for strip access.** On a second isolated thread, I added 18 long-named files and double-clicked their tree rows to keep all 18 inner tabs. Four tab labels fit in the Files header at the test width, with no inner-strip scroll or overflow controls. A debug lookup placed `files-tab-17` at logical x=3250, outside the 1440-pixel window. The file tree still allows activating a path, so this does not make the file contents unreachable by every route; it does make the clipped tab strip and its hidden close controls inaccessible through that strip.

Evidence: [18 kept tabs](../evidence/files-correction/26-kept-tabs-overflow.png). The initial populated tree is [here](../evidence/files-correction/24-files-thread-two.png).

### Inner-tab restore after changing outer surfaces

**Status: inconclusive.** I switched from Files to Overview and back. The Files outer surface reappeared, but the headless screenshot showed a blank body, including after another capture. I cannot determine from this render whether the selected inner tab was restored or whether the renderer failed to draw it. See [first capture](../evidence/files-correction/28-files-restored-after-switch.png) and [follow-up capture](../evidence/files-correction/29-files-after-switch-settle.png).

### Save conflict and save failure

**Not checked.** The existing Files editor typing focus issue redirects typing to the main composer. I did not establish a dirty editor buffer through public controls, so I did not exercise Save, close-with-dirty-buffer, write failure, or conflict recovery. This result is not a pass or a reproduction of those findings.

## Cleanup

Both scratch threads were closed. The audit server PID 43091 and headless PID 43792 were sent SIGTERM by process group. Follow-up checks found both PIDs and process groups absent, and `ps -axo pid,pgid,command` returned no process containing the scratch root path. Exact cleanup details are recorded in [cases.json](../evidence/files-correction/cases.json) and the final command records in [commands.jsonl](../evidence/files-correction/commands.jsonl).
