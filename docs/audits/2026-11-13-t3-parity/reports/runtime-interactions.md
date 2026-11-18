# Isolated GPUI interaction audit

The user wants production-quality evidence of broken everyday Ferrite workflows, including right-panel focus, ordinary keyboard behavior, citations and link menus. This slice exercises the rendered production Cockpit through GPUI pointer and key dispatch. A callable handler is not the success criterion: the intended control must receive the input, the visible result must match the action, and sends must reach only the intended destination.

## Result and scope

**ISSUES.** This run adds runtime evidence to seven existing findings, without adding duplicate issue IDs. Files editor/filter clicks redirect ordinary typing into the main composer; Find after clicking a file searches the transcript; Enter in the empty panel sends the main draft. Citation tiles have no comment editor. Inline file-chip context menus expose whole-message actions. Ordinary web-link menus have no explicit system-browser choice and remain open after Escape.

Two user-reported symptoms did **not** reproduce within the specific cases tested. A clean double-click selection followed by Cite created one quote, immediately accepted general composer text, and sent the exact quote-plus-text payload. An ordinary HTTP(S) transcript link offered both Open link and Copy link. These results do not settle the installed app's intermittent Cite failure or its reported copy-only menu in other link contexts.

The JSON companion is an empty array because these observations belong to existing issue IDs. [Structured observations](../evidence/headless/observations.json) record the evidence upgrades and limits; the owning reports retain their complete cause analysis and T3 comparisons.

## Exact environment and isolation

| Item | Recorded value |
| --- | --- |
| Ferrite source | `62947f91d5cd6ee494aaae9ed09d794a853e5bab` |
| Build | Parent ran `cargo build --locked -p ferrite --features headless`; exit 0, dev profile, 14m03s. This worker waited for explicit completion and ran no builds or tests. |
| Executable | `/Users/josephwylie/Desktop/Projects/ferrite/target/debug/ferrite` |
| Executable SHA-256 | `8f47873505f48c90a267ef5b02913a345b72e390d17b78cd3a72c552497a2b98` |
| T3 comparison source | `74573fc29b901bafbf8f1dbb2eb29606da1fa448`; source comparison only, no T3 runtime exercise in this slice |
| Fresh store | `/tmp/ferrite-audit-runtime-g7gkzsau/threads` |
| Fresh checkout fixture | `/tmp/ferrite-audit-runtime-g7gkzsau/work` |
| Thread | Isolated thread 1, title Runtime audit fixture, fake Claude peer |
| Render | 1440 × 900 logical units; `window.shot` saved 2880 × 1800 PNGs at scale 2 |
| Server | PID/process group 46336 |
| Headless window | PID/process group 46369 |
| Fake provider | Session PID 47726; discovery process PID 46676 |

[state.json](../evidence/headless/state.json) records the binary identity and process ownership. The wrapper removes inherited `FERRITE_*` variables, sets the fresh store, runs from the fixture directory, and prepends temporary provider shims to PATH. Updates and start-at-login are disabled in the temporary settings. No operator store or installed window was attached. Application source, dependencies and tests were unchanged.

The fake Claude peer advertises version `9999.0.0` because provider discovery selects the newest candidate; putting a lower-version shim first on PATH is insufficient. The session launch and every user frame are recorded in [provider-launches.jsonl](../evidence/headless/provider-launches.jsonl) and [provider-host.jsonl](../evidence/headless/provider-host.jsonl). Real installed CLIs could receive discovery `--version` calls; all prompts in this run reached the recorded fake peer. No remote model completion was requested. The fake response was:

```markdown
The amber fox crosses the quiet bridge.

Read [fixture documentation](https://example.invalid/ferrite-audit) and [sample.rs](sample.rs).

Selection should remain available while you add your own annotation.
```

## Method, key concepts and source map

`window.click` resolves a rendered selector to its geometric center or accepts observed logical coordinates, then dispatches pointer move/down/up events. `window.key` and `window.type` dispatch keystrokes to the currently focused GPUI control. They do not call Files editing, panel-launcher or Cite handlers directly. Thread creation and the first fake response use the server API only to establish the fixture. All tested interactions thereafter use the rendered window.

| Boundary | Source and significance |
| --- | --- |
| Production view and keymap | `crates/ferrite-cockpit/src/headless.rs:27-101`: fresh-store guard, settings/keymap loading, `CockpitView::new_with_settings`, production server bridge |
| Driver actions | `crates/ferrite-cockpit/src/headless.rs:104-162`: key, type, pointer, geometric find, screenshot |
| Actual keys | `crates/ferrite-cockpit/src/headless.rs:283-315`: `window.dispatch_keystroke`; individual keys settle for 60 ms |
| Actual pointers | `crates/ferrite-cockpit/src/headless.rs:340-383`: selector bounds, move, mouse-down and mouse-up |
| Screenshot settling | `crates/ferrite-cockpit/src/headless.rs:386-394`: six 60 ms ticks before capture |
| Focus reconciliation | `crates/ferrite-cockpit/src/cockpit.rs:10159-10162,10220-10274`: root focus ownership |
| Input ordering | `vendor/gpui-pre/src/window.rs:6152-6168`: action matching precedes raw key-down behavior |
| Cite delay | `crates/ferrite-pane/src/transcript/reply.rs:59-95`; `crates/ferrite-ui/src/theme.rs:1973`: 500 ms after multi-click selection |

A driver response of `unhandled: []` means some control/action consumed the key. It does not prove the intended control received it. The screenshots, file contents and fake-provider frames distinguish those outcomes here.

All 105 application commands, their timestamps, store, exit codes and responses are preserved in [commands.jsonl](../evidence/headless/commands.jsonl). Line references below are JSONL record numbers. PNGs cited below were opened with `view_image`, not accepted solely because the capture command succeeded. Some intermediate screenshots contain black or stale-looking regions; this run makes no rendering or performance defect claim from those captures.

To read the command examples below, `drive.py run` invokes the recorded binary with the isolated environment. The full path is `docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py`. The launch helpers are [drive.py](../evidence/headless/drive.py) and [provider-peer.py](../evidence/headless/provider-peer.py). They are evidence fixtures, not application changes. `prepare` intentionally refuses to overwrite an existing state file; reproducing from scratch requires a new evidence directory/state and fresh temporary store.

## Traced interactions and evidence upgrades

### input-focus-010: Files typing reaches the main composer

Priority P1, defect. **Runtime-reproduced in the isolated production Cockpit.** This confirms the source-backed focus redirection, with an earlier onset than the original report's first-edit example: in this run even the first typed character after the pointer click appeared in the composer.

Fixture: `sample.rs` contains `pub fn amber() {`, `    let value = 42;`, and `}`. Files is open beside the transcript. Commands 5-20:

```sh
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key cmd-shift-f
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector files-row-1
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --x 655 --y 136
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key x
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key y z
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key cmd-a backspace
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector files-filter-placeholder
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key s
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key a m
```

Expected: `xyz` edits the clicked file; `sam` filters the file tree. Actual: the main composer shows `xyz` after the editor sequence and `sam` after the filter sequence. The file remains unchanged, the filter still displays its placeholder, and README.md and sample.rs remain visible. No save action was required to establish the wrong input destination. The on-disk original is preserved as [sample-after.rs](../evidence/headless/sample-after.rs).

Evidence: [02-editor-before.png](../evidence/headless/02-editor-before.png), [03-editor-first-key.png](../evidence/headless/03-editor-first-key.png), [04-editor-next-keys.png](../evidence/headless/04-editor-next-keys.png), [05-filter-first-key.png](../evidence/headless/05-filter-first-key.png), [06-filter-next-keys.png](../evidence/headless/06-filter-next-keys.png). The strongest comparisons are 02→04 and 06's main text versus unchanged filter.

Cause remains the source-backed missing Files exemption, not a focus-handle trace recorded by this runtime session. See `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:301-312,201-216`; `crates/ferrite-cockpit/src/cockpit/tools/files.rs:125-135,228-239,446-464`; `crates/ferrite-cockpit/src/cockpit.rs:10159-10162,10220-10274`. The direct `Editor.type_text` test at `crates/ferrite-cockpit/src/cockpit/tests/files.rs:74-89` does not exercise this key destination.

Recommendation and acceptance: make focused Files controls explicit owners across root redraws. Repeat the pointer-and-key sequence with a multi-character edit, filtering, a background update and a later save; assert only the intended control changes and the main draft remains byte-for-byte unchanged. Full issue and comparison: [input-focus-010](input-focus.md#input-focus-010-files-editor-and-filter-lose-keyboard-focus-when-their-edits-redraw-the-cockpit).

### input-focus-009: Find after clicking file contents searches the transcript

Priority P2, defect. **Runtime-reproduced for the natural click-file → Find journey.** Commands 21-25 clear the main draft, click the file at `(655,136)`, press `cmd-f`, and type `amber`.

Expected: matches in the displayed file. Actual: the transcript Find UI opens and highlights the assistant's `amber`; the file's `amber` is not highlighted. [07-file-find.png](../evidence/headless/07-file-find.png) shows both regions. The concurrent focus-loss defect above may participate in this particular runtime path; this run does not establish that the editor retained focus before dispatch. The independent source absence of a Files Find owner remains documented in `crates/ferrite-cockpit/src/cockpit.rs:6144-6164`, `crates/ferrite-cockpit/src/cockpit/tools/files.rs:271-293`, and `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:426-504`.

Recommendation and acceptance: route Find through the focused surface. Repeat with a file-only sentinel absent from the transcript, then with the same word in both regions; the file must own both search and Escape return. Full issue: [input-focus-009](input-focus.md#input-focus-009-find-in-a-files-editor-searches-the-conversation-instead-of-the-file).

### input-focus-002: Empty-panel Enter sends the main draft

Priority P1, defect. **Runtime-reproduced through key dispatch and the provider boundary.** This is the strongest consequential failure in this slice: the user points at a launcher, sees its Files row selected, and Enter submits unrelated composer text.

Commands 30-39:

```sh
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector panel-tab-Files-close
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector panel-tab-Overview-close
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window type PANEL_ENTER_SENTINEL
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector panel-empty
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key down down
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key enter
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key f
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window find panel-tab-Files
```

Expected: Enter opens highlighted Files and leaves the main sentinel unsent. Actual: [09-panel-before-enter.png](../evidence/headless/09-panel-before-enter.png) shows the sentinel in the composer and Files highlighted. [10-panel-after-enter.png](../evidence/headless/10-panel-after-enter.png) shows the sent sentinel and still-empty launcher. [provider-host.jsonl:6](../evidence/headless/provider-host.jsonl) records a real user frame with exactly `PANEL_ENTER_SENTINEL`. The subsequent bare `f` opens Files, and geometric find returns its tab at x584/y10, width58/height24. That control check supports the launcher owning raw keys while Enter resolves to a root action.

Cause: the launcher uses raw Enter handling without an overriding key context; GPUI's root action consumes Enter first. `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:670-691,716-742`; `crates/ferrite-ui/src/keymap.rs:185-189`; `vendor/gpui-pre/src/window.rs:6152-6168`. The existing test calls the helper directly: `crates/ferrite-cockpit/src/cockpit/tests/tools.rs:190-220`.

Recommendation and acceptance: give the launcher an explicit Enter action/context and guard main submission by intended ownership. Preserve the sentinel test as an end-to-end key-dispatch check; zero provider frames may be sent until the composer itself is deliberately submitted. Full issue: [input-focus-002](input-focus.md#input-focus-002-enter-in-the-empty-panel-invokes-the-conversation-submit-action-instead-of-the-highlighted-opener).

### quote-attachments-001 and quote-attachments-012: General text works; a per-quote comment editor is absent

Existing `quote-attachments-001` is P1, missing-capability; **runtime evidence supports the absent comment editor**, with the source inspection establishing that no alternate comment mutation path exists. Existing `quote-attachments-012` remains a verification gap for the user's broader installed-app pointer regression.

The clean run removes exploratory quotes and text, then uses commands 66-80. The panel is collapsed. Double-click `(540,163)` selects `amber` in the first answer. After the multi-click settling delay, clicking the visible Cite button produces one quote. Geometric find succeeds for `tray-reply-0` and fails for `tray-reply-1`. Immediately typing `My clean annotation.` puts that text into the main composer. No additional focus click was needed for this ordinary text entry.

```sh
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector composer-row
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key cmd-a backspace
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --x 540 --y 163 --double
# The recorded screenshot and command round trips allow the 500 ms selection delay to settle.
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py shot 20-cite-clean-selection.png
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector selection-reply-quote
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window type 'My clean annotation.'
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector tray-reply-0
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key q
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window click --selector composer-row
python3 docs/audits/2026-11-13-t3-parity/evidence/headless/drive.py run window key enter
```

Clicking the quote tile opens no editor. The subsequent `q` is explicitly reported as unhandled. [21-clean-cite-and-annotation.png](../evidence/headless/21-clean-cite-and-annotation.png) shows the single quote tile and general text; [22-clean-tile-no-editor.png](../evidence/headless/22-clean-tile-no-editor.png) shows the unchanged tile after attempted comment input. After clicking the composer and Enter, [23-quote-sent.png](../evidence/headless/23-quote-sent.png) shows Quote amber and My clean annotation. as separate display regions. The fake peer received exactly:

```text
> amber

My clean annotation.
```

The string is in [provider-host.jsonl:7](../evidence/headless/provider-host.jsonl). This is positive evidence for one normal selection → Cite → general text → send flow, not evidence that an editable annotation is associated with a particular citation. The data model's `Reply.text` support and preloaded fixture comments do not provide a user editor: `crates/ferrite-pane/src/transcript/reply.rs:206-220`; `crates/ferrite-pane/src/attachments.rs:551-618`; `crates/ferrite-pane/src/composer.rs:409-419,645-654`.

Exploratory captures 12-18 are retained for transparency but are not the clean success criterion. The first post-double-click screenshot preceded the 500 ms toolbar delay; the next showed Cite. Later exploratory pointer presses overlapped an already-open toolbar and produced two quote tiles. This run does not claim a duplicate-Cite bug from that sequence. The reset and single-tile check above remove that ambiguity.

Recommendation and acceptance: provide an explicit per-citation comment editor with save/cancel and later editing. Use two different citations with different comments plus general text, then reopen and send; verify each comment stays paired with its source. Reproduce the user's installed-app failure separately with drag, multiline and streaming selections. Full findings: [quote-attachments-001](quote-attachments.md#quote-attachments-001-cite-has-no-reachable-per-quote-comment-editor), [quote-attachments-012](quote-attachments.md#quote-attachments-012-the-reported-pointer-cite-regression-still-needs-installed-app-reproduction).

### link-affordances-01: Web-link menu has Open and Copy, without a system-browser choice

Priority P2, missing-capability. **Runtime-reproduced menu composition; destination routing remains source-proven.** After dismissing any prior message menu by a blank click, commands 92-95 right-click the visible fixture documentation link at `(565,238)`. [28-web-menu-actual.png](../evidence/headless/28-web-menu-actual.png) shows exactly Open link and Copy link. There is no explicit system-browser action.

This narrowly contradicts the user's copy-only observation for an ordinary Markdown HTTP link in this source build. It does not identify what link context or installed version produced that report. No browser action was activated, so this run does not claim URL loading, OS handoff or copy contents were verified. Source routing is `crates/ferrite-pane/src/transcript/message_menu.rs:132-139,192-198` and `crates/ferrite-pane/src/preview.rs:322-331`.

Recommendation and acceptance: offer separately named integrated/system destinations and verify both with the same exact URL. Keep the user's original copy-only case open until its surface and installed version are known. Full issue: [link-affordances-01](link-affordances.md#link-affordances-01-transcript-web-link-menus-cannot-explicitly-open-the-system-browser).

### link-affordances-04: Inline file-chip right-click opens the whole-message menu

Priority P2, partial-capability. **Runtime-reproduced.** Commands 96-98 dismiss the web menu and right-click the displayed `sample.rs` chip at `(744,238)`. [29-file-chip-context-menu.png](../evidence/headless/29-file-chip-context-menu.png) shows Copy message, Copy message as Markdown, Attach message as context, Pin as chapter and Fork from here. No file path, file open, editor or reveal action is offered. This settles the owning report's uncertainty about the fallback for this actual chip.

By contrast, right-clicking `files-row-1` in the Files tree did show Attach as context, Copy submenu and Show in Finder: [08-file-tree-menu.png](../evidence/headless/08-file-tree-menu.png), commands 27-28. The tree menu's visible presence is positive coverage, not evidence that every action or submenu works.

Cause: custom chip rendering loses native link target metadata and has no right-click target handler. `crates/ferrite-pane/src/rich.rs:567-575`; `vendor/gpui-base/src/text/inline_flow.rs:193-233`; `crates/ferrite-pane/src/attachments.rs:795-855`; `crates/ferrite-pane/src/transcript/message_menu.rs:269-284`. Recommendation and acceptance: preserve a typed file target through custom rendering; right-click must offer path/open/reveal actions for that file, including a line suffix where present, independent of the previously opened link menu. Full issue: [link-affordances-04](link-affordances.md#link-affordances-04-custom-file-chips-lose-target-aware-context-actions).

### link-affordances-06: Escape does not close the transcript web-link menu

Priority P2, interaction. **Runtime-reproduced for Escape dismissal only.** Commands 99-103 dismiss the file menu with a blank click, right-click the web link again, press Escape, find `answer-menu`, and capture [30-web-menu-after-escape.png](../evidence/headless/30-web-menu-after-escape.png). The menu remains visible. Find returns x565/y238, width160/height66. The driver reports `unhandled: []`, so another action consumed Escape without closing this menu.

This supports the keyboard-menu portion of the existing finding; link Tab reachability, arrows and Enter were not exercised here. The source rows install mouse interaction without a keyboard menu owner: `crates/ferrite-pane/src/transcript/message_menu.rs:132-139,192-203`, and the broader source trace in [link-affordances-06](link-affordances.md#link-affordances-06-ordinary-markdown-links-and-their-transcript-menus-have-no-keyboard-activation-path).

Recommendation and acceptance: use a focus-owning menu with Escape dismissal and focus restoration, then verify arrow/Enter navigation and keyboard invocation on the link itself. A handled key must correspond to the visible menu action, not unrelated root behavior.

## Supported comparisons and positive coverage

The relevant T3 source comparison remains bounded: this worker did not run T3. Its current source provides [per-citation comment editing](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L81), [separate integrated/system web-link menu verbs](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/externalLinkContextMenu.ts#L111-L124), [target-aware file context actions](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownFileLink.tsx#L247-L309), and [a semantic empty-panel launcher](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L438-L460). These are source-backed capabilities, not claims that every T3 runtime edge case passes.

Ferrite behavior positively observed here: Files opens via its shortcut, a tree row opens the file, a Files tree context menu is visible, panel tabs close, raw launcher `f` opens Files, double-click text selection exposes Cite after its documented delay, one clean Cite produces one tile, immediate general text entry works in that case, quote and general text reach the fake provider together, ordinary web links expose Open and Copy, and clicking outside a transcript menu dismisses it.

## Verification gaps and limits

- The offscreen GPUI run validates the production view and event dispatch, not macOS Accessibility, native window activation, system clipboard, IME, trackpad behavior or the installed app binary. The parent owns separate native evidence.
- The exposed driver supports single/double/right click with default modifiers. This run cannot establish drag selection, mouse-wheel behavior, resize drag, middle-click, modifier-click or pointer capture through those commands. No source changes were made to expand the driver.
- The fake provider replies immediately with fixed short text. Streaming reflow, long transcript virtualization, approvals, network errors, real provider behavior and concurrent subagents remain untested.
- No CEF page was loaded and no OS browser/editor/reveal action was activated. Web menu labels and file-chip fallback are verified; their successful destinations and clipboard values are not.
- Cite coverage is one short word selected by double-click, not the full drag/multiline/code/table/streaming matrix. The reported installed-app inability to cite or add text remains unresolved beyond this bounded success.
- No motion smoothness or performance claims follow from settled screenshots. Black/stale-looking regions in intermediate captures were not promoted to findings.
- The broader link keyboard finding is only runtime-confirmed for Escape dismissal; Tab, arrow and Enter behavior remains source evidence.
- This audit did not run a unit suite, typecheck or additional build. The parent-built headless binary and actual interaction sequence are the verification performed here; no application fix is proposed or certified.

## Cleanup and retained evidence

Commands 104-105 close isolated thread 1 and verify `thread list` returns an empty array. The wrapper then sends SIGTERM to its owned headless and server process groups. A separate process-table check verifies server PID 46336, headless PID 46369, session peer PID 47726, discovery peer PID 46676, and both owned groups are absent. [cleanup.json](../evidence/headless/cleanup.json) records the check with `verified_clean: true`; [state.json](../evidence/headless/state.json) records `still_exists: false` for both long-running owned processes.

The temporary fixture/store and audit artifacts are retained to inspect the precise input/output state. All created long-running processes are stopped. No operator process was signaled. The next work should first repair the unintended-send and focus-ownership defects, then verify quote comment authoring and target-aware menus through the same real event paths. Existing helper tests should not be counted as protection for these particular dispatch failures.
