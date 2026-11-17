# Existing-workflow coverage correction

The audit does **not** establish that Ferrite is production quality. It now has 52 explicit existing-workflow contracts, covering every one of the original 132 repair-quality findings. **Zero complete journeys are certified.** That means the evidence does not cover each complete journey; it does not mean every action in the app fails. The primary agent has added four distinct repair records to the original inventory, producing 176 consolidated records; all 136 current repair-quality records map into these same journeys.

This corrects the audit's method: a component, command, surface tab, source implementation, or passing helper test does not prove that a person can finish the task. Clicking a PR badge must display that PR; opening an agent must show its current output; opening Files must let ordinary typing edit the file; closing and reopening a panel must preserve useful geometry and working state.

The matrix below owns the remaining work. The primary audit agent owns every follow-up without a specialist owner. The user is not responsible for discovering or enumerating these cases. A specialist assignment is not a verification result.

## What was actually exercised

The first runtime pass exercised a short deterministic response, a few Files controls, the empty-panel launcher, one short citation, and a few context menus. It did not exercise complete PR navigation/review, long or live output, nested agents, queue editing, saving after failures, attachment lifecycle, remote settings, or restart recovery. The previously passed 12 narrow scroll tests remain useful evidence for their assertions; they were not rerun here and cannot clear the nine broader scroll findings.

The completed panel correction now adds a real multi-step failure: Overview begins at 320 logical pixels; visiting Files expands it to 864 of a 1440 px window, hides the left navigation, and leaves 576 px for the conversation. Returning to Overview, closing the wide tab, closing/reopening the panel, and restarting retain 864 px. A separate fresh thread reproduced the same effect with Changes. [Panel report and measured sequence](panel-runtime-correction.md). This is direct evidence for the user's oversized-panel complaint; deliberate resizing and small-window behavior still need their own checks.

The earlier runtime evidence also establishes narrow failures: Files/filter typing went into the main composer; Files Cmd-F opened transcript search; Enter in the launcher submitted the composer; an individual quote had no comment input; a file reference had message actions instead of file actions; a web menu stayed open on Escape. One short cite-plus-general-text send succeeded. [Prior runtime report](runtime-interactions.md). None of these outcomes licenses a blanket claim about every link, citation, file, or input route.

## Reconciliation snapshot

All completed correction reports through navigation, including the read-only Working rename clarification, are reconciled. Four parent-adopted quality-correction records map into existing journeys. Forms valid input, blank paging and Working rename commit remain inconclusive within their stated cases; no complete workflow is certified.

Current status: **23 not exercised, 22 partial runtime failures, 5 positive branches, 2 inconclusive runtime attempts; zero complete journeys certified.** These are coverage states, not a production quality score.

- Panel: two fresh threads reproduce compact Overview becoming permanently wide through Files/Changes; reopen and app restart retain it. Native divider/narrow-window/motion remain open. [Report](panel-runtime-correction.md).
- Agents: direct child opening and Main/child incremental visibility work in the controlled protocol; nested opening, the separate live-output route, blank reopen and ordinary-width prose clipping fail. Synthetic final Main duplication is excluded because its fixture lifecycle was invalid. [Report](agent-live-correction.md).
- Visuals: inspected rendered chrome shows redundant browser hierarchy, weak empty address affordance and irrelevant host-error recovery advice. Parent corroborates hierarchy in loaded native Browser without a saved capture path; native loading/focus/error/motion states remain open. [Report](visual-quality-correction.md).
- Drafts: a never-sent text draft disappears after window-process restart; queued Edit merges distinct queued/current drafts; SideChat independently reproduces blank reopen. Normal Quit, multi-item races and completed SideChat recovery remain open. [Report](draft-lifecycle-runtime-correction.md).
- Forms: required validation, Cancel and explicit Deny succeed. Valid input remains inconclusive: selectors identify parent wrappers, exact Input hit is unproven, and headless paint/clipping is ambiguous. No new missing-field or wrong-focus defect is certified. [Updated report](forms-runtime-correction.md).
- Scroll: folded loaded tool output returns no matches until disclosure; latest-answer Find and ordered appends work. Blank paging captures also lose unrelated chrome, so their cause remains inconclusive pending independent evidence. [Report](scroll-runtime-correction.md).
- Quotes: 990-character whole-context source plus commentary sends/restores intact; preview/full-source inspection and per-quote editing fail. 10,000-character boundary and ordinary leading general-editor blockquotes are not covered. Actual file/image intake remains unexercised. [Report](attachment-runtime-correction.md).
- PRs: actual B badge leaves A selected; focused badge opens Browser while Overview opens populated native detail. Refresh leaves same-PR details stale; re-entry and lifecycle-change controls recover. Writes and native GitHub page load remain untested. [Report](pr-entry-runtime-correction.md).
- Files: clean preview and directory children stay stale after external changes; inner tab strip clips offscreen tabs. Dirty save/conflict is untested, long-diff end reachability is untested, and blank outer-switch restore is inconclusive. [Report](files-runtime-correction.md).
- Navigation: broad search can Enter-open a selected result above the viewport; a visible keyboard target works. Snoozed Rename has no editor. Working/settled show hints, but Working exact Input/commit and settled save were not established; the immediate Working title read proves no persistence defect. Parked Rename and project/draft continuity remain open. [Updated report](navigation-runtime-correction.md).

## Original-record reconciliation

Original-ID recommendations attach bounded runtime observations. Broad combined original records may retain source evidence levels; no blanket reclassification is required or implied.

- `panel-tools-018` — runtime reproduced: width remains enlarged after wide-tool visit, tab close, reopen and restart. **Exclude:** narrow-window and manual divider behavior; input-focus-008 not upgraded.
- `subagents-background-02` — runtime reproduced: nested row and View live output controls inert. **Exclude:** all alternate ancestry routes and parent/root navigation.
- `subagents-background-03` — runtime reproduced separately for Agent and SideChat single-tab close/reopen. **Exclude:** Close all, completed SideChat reply, real-provider continuity, Cmd+W, restart and evicted history.
- `subagents-background-08` — bounded runtime refinement: child row live-output link opens Background with false No output yet. **Exclude:** complete background inventory, mixed command/agent and stopping behavior.
- `BROWSER-15` — visual/runtime evidence: unavailable-host error appends irrelevant generic recovery advice. **Exclude:** native invalid-address and network-error states.
- `BROWSER-08` — none; loading geometry remains source-backed. **Exclude:** settled chrome screenshots do not establish loading movement.
- `motion-001` — none; user report and source snap policy remain, no timed intermediate frames. **Exclude:** endpoint screenshots cannot prove transition timing.
- `composer-02` — runtime reproduced: one fresh unsent plain-text draft missing after window process termination/restart despite autosave delay. **Exclude:** normal Quit, multiple drafts, attachment/model/checkout recovery and promotion.
- `composer-04` — runtime reproduced: actual Edit cancels one queued UUID and merges unrelated draft. **Exclude:** two-item order, resubmission, race and Save/Cancel workflow.
- `coverage-quality-03` — no runtime defect promotion: valid-answer coverage remains unresolved, while empty validation, Cancel and Deny work. **Exclude:** parent wrapper bounds are not nested Input bounds; missing-field/wrong-focus defect is not established; input-focus-007 and PS-01 remain untested.
- `transcript-001` — runtime reproduced: folded loaded Read output returns no matches; disclosure permits a match. **Exclude:** all folded kinds and active-result anchoring.
- `quote-attachments-001` — runtime evidence reinforced: whole-context tile click offers no per-quote comment editor. **Exclude:** general composer commentary works; no blanket Cite failure.
- `quote-attachments-002` — runtime reproduced: 88×88 quote tile and sent row cannot disclose complete 990-character source. **Exclude:** provider receives complete content; no payload loss.
- `quote-attachments-004` — none; successful fixture is 990 trimmed characters, below 10,000 limit. **Exclude:** boundary refusal remains untested.
- `quote-attachments-005` — none; tested nested source quote is inside deliberate context. **Exclude:** literal leading Markdown in general composer remains untested.
- `pr-navigation-01` — runtime reproduced twice: B badge click leaves A selected with no visible panel. **Exclude:** B absent/empty board, footer/modifier routes.
- `pr-navigation-02` — runtime confirms split entry: badge opens Browser; Overview opens populated native review. **Exclude:** native CEF page load and all entry variants.
- `pr-operations-001` — runtime reproduced: same-identity/lifecycle refresh leaves details stale despite fresh Overview CI. **Exclude:** agent-action branch untested; lifecycle change and explicit re-entry do refresh.
- `panel-tools-003` — runtime reproduced: cached directory children stay stale through collapse/re-expand. **Exclude:** Git status refresh and discovery caps.
- `panel-tools-005` — runtime reproduced: 18 kept inner tabs clip with offscreen tab 17 and no strip overflow control. **Exclude:** tree path activation remains usable; restore unproven.
- `panel-tools-002` — supporting clean-cache staleness only: open Markdown preview ignores external write until close/reopen. **Exclude:** dirty conflict and destructive overwrite not exercised.
- `panel-tools-009` — no full runtime promotion: full diffstat/initial hunk seen; 120-row cap is source-backed. **Exclude:** scroll to omission line and separate Review route untested.
- `projects-navigation-006` — bounded runtime support: query starts with cursor 0 above retained scroll offset and Enter opens unseen 02; visible-target keyboard control works. **Exclude:** arrow movement beyond viewport and live result reordering are untested.
- `projects-navigation-001` — runtime support for Snoozed only: offered Rename closes menu without drawing editor. **Exclude:** Parked Rename untested; Working/settled completed save not established.

The primary agent adopted four distinct correction records, now mapped to these existing journeys without certifying the whole workflow:

- `quality-correction-001` / `AGENT-01` — Agent transcript prose clips at ordinary panel width.
- `quality-correction-002` / `BROWSER-UI-03` — Browser retains a duplicate heading, nested tabs and legacy card beneath shared panel tabs.
- `quality-correction-003` / `BROWSER-UI-03` — Empty browser address entry lacks a clear editable-region affordance.
- `quality-correction-004` / `NAV-02` — Native PR read errors run beyond the panel and hide their cause and recovery text.

The primary agent owns the original inventory, adjudications and tally. This matrix does not modify them. All untested branches below remain audit work, including those assigned to a specialist whose first fixture has finished.

## Completion contract

For each journey, exercise the visible entry, ordinary mouse and keyboard operation, asynchronous update, error/recovery, leaving the surface, and returning or restarting where applicable. Verify the destination identity, retained data, visible feedback, focus ownership, and reading position across surfaces. A failure at any step keeps the journey open. A precondition established by an internal helper must be disclosed; it cannot count as proof of the user-facing entry path.

Use distinct evidence labels:

- **Not exercised:** source review identifies risk, but runtime has not exercised this complete path. This is neither a runtime failure claim nor a pass.
- **Partial runtime failure:** at least one bounded executed branch fails. Remaining branches still require verification.
- **Positive runtime branch only:** one bounded branch succeeded. Its surrounding journey remains unverified.
- **Runtime inconclusive:** a runtime attempt did not establish the intended input/rendering boundary. Preserve observed successes and confounds; certify neither a product defect nor the complete workflow.

There is no production-quality percentage here. The denominator mixes different amounts of work, and a single wrong-target or data-loss failure can make a whole workflow unusable.

## Remaining verification, in priority order

Navigation search and Snoozed rename have now been exercised with the bounds below. The remaining checks are these unverified branches rather than repeats of already demonstrated failures:

1. **BROWSER-UI-01, BROWSER-UI-02, BROWSER-UI-03, SHELL-01, SHELL-02** — Native Browser keyboard/mouse ownership, popups/dialogs, loading/error/focus states, actual panel resize and timed sidebar open/close/reversal. Headless evidence cannot substitute for these.
2. **READ-02, READ-03, READ-04** — Resolve headless repaint confound with semantic viewport state or native capture, then establish a known visible reading anchor before live append, disclosure, width change, scrollbar input, End and Latest.
3. **FILES-01, FILES-02, DIFF-01, DIFF-02** — Establish actual editor input, then dirty save failure/conflict and retained edits. Use native scroll to inspect complete diff and full Review; clean preview/tree staleness is already reproduced.
4. **ATTACH-01, ATTACH-02, QUOTE-01, QUOTE-03** — Native file/image picker or drop and supported formats, inspect/remove/send/history; multiline Cite, 10,000/10,001 context boundary and ordinary leading Markdown in the general editor.
5. **PERM-01, AGENT-02, AGENT-03, AGENT-04** — Establish exact form Input hit and valid payload; child decisions/Stop target IDs, mixed-task inventory, completed SideChat history, eviction, restart and real provider event compatibility.
6. **COMPOSE-01, COMPOSE-02, COMPOSE-03, NAV-02, NAV-03, NAV-04, RESTORE-01** — Normal Quit and multi-draft promotion/context restore; queued Edit order/races; PR read error/concurrency/complex review and controlled action lifecycle; coherent mixed-surface restoration.

7. **NAV-05, NAV-06, NAV-07** — Search after live reorder/disappearance, actual offscreen keyboard selection, visible Parked rename, completed Working/settled rename, failed reopen, grouped Undo Park, bulk failures and draft/project/checkout continuity.

## Initial runtime fixture recipes

Completed runtime reports are listed above. The recipes below preserve the initial high-priority fixture handoffs; the case-specific next actions and untested branches supersede their original coverage status. They use a fresh owned fixture and actual rendered input, and should preserve provider frames, before/after state, screenshots, and cleanup proof. The listed CLI fragments are arguments for an owned driver's `run` mode; they are handoff commands, **not commands executed by this matrix author**.

1. **Queued Edit with a separate unsent draft — COMPOSE-03.** Use the busy staged provider pattern in `evidence/agent-live-correction/provider-peer.py`. While busy, dispatch `window type QUEUED_SENTINEL`, `window key enter`, then `window type UNRELATED_DRAFT` and `window click --selector queued-edit-0`. Compare the queued item, composer text/attachments, queue identity and actual provider frames. Exercise Cancel, two items, and a late queue acknowledgement. The selector is rendered by `crates/ferrite-pane/src/pane/composer.rs:927`; the existing queue tests are setup references, not end-to-end clearance.
2. **Never-sent draft navigation/restart — COMPOSE-02.** Dispatch `window key cmd-n`, enter a unique unsent draft, switch away/back, allow the autosave boundary, then restart only that owned window against the same Store. Preserve text, quotes, attachments and caret expectations separately. Existing `tests/drafts.rs` starts with created thread IDs and does not establish this never-sent pane path. A new pane without a thread ID is precisely the missing branch.
3. **PR badge wrong-target and no-op — NAV-01/02.** Create two scratch Git checkouts and a PATH-scoped `gh` fixture whose response depends on cwd. Production discovery is `crates/ferrite-core/src/workspace/mod.rs:719–765`, calling `gh pr view --json number,state,isDraft,url,title,statusCheckRollup`. Focus A, preserve A's draft, and click B's actual rendered PR badge with the panel closed. Assert B's PR identity and native destination. Then inject empty, delayed and failed reads. `tests/pr.rs` uses `pr_landed`/`fixture_pr_snapshot`; direct injection bypasses the badge-to-discovery path and cannot clear it.
4. **SideChat close/reopen and close ownership — AGENT-03/SHELL-04.** SideChat has a distinct backing resource from agent tabs. Enter through its public command, produce content, close via the actual tab close or Cmd-W, use Command Palette `Reopen closed surface`, and check content plus provider launch count. Do not assume a reopened tab means its model/history was hydrated. `crates/ferrite-cockpit/src/cockpit/palette.rs:932` exposes the reopen command. Existing `tests/side_chat.rs` commonly opens it through an internal helper.
5. **Required answer forms and focus — PERM-01.** Send a controlled provider form with required fields, focus it by click, type the answer, navigate with Tab, and submit with ordinary input. Confirm the response belongs to that request, the main composer is unchanged, and invalid answers remain visible/recoverable. `tests/provider_forms.rs` supplies protocol shapes, but any direct field mutation must be replaced by public input for this check.
6. **File save failure/external modification — FILES-01/02.** First prove the ordinary editor input path. Then use one scratch file and deterministic write rejection, or externally modify it while open; assert unsaved text survives close/reopen and no newer disk contents are silently overwritten. Run only inside owned scratch storage. A helper can establish a dirty buffer for diagnostic isolation, but cannot make the end-to-end journey pass while actual keyboard routing fails.
7. **Find through folds and navigation during live growth — READ-01/02/03.** Use a long deterministic transcript with the same sentinel in collapsed and expanded results, plus a gated live append. Search, select the hidden hit, Escape, manually scroll away, append, collapse/expand, resize, and use End/PageUp/PageDown. Verify the visible target and anchor after each transition. The headless public API has no wheel or window-resize command; those branches belong in native UI or a newly documented harness seam, not a false headless pass.
8. **Changes completeness and restored working sets — DIFF-01/02, FILES-03/04, RESTORE-01.** Build large scratch trees and diffs across existing display limits, keep enough tabs to overflow, change files externally, then restart. Check all data is reachable, stale state is identified, scroll position/selection remains useful, and the existing Git action reports its actual result. A plausible first screenshot is only the entry state.

The existing driver implementation is retained at [agent driver](../evidence/agent-live-correction/drive.py) and [panel driver](../evidence/panel-correction/drive.py). Those evidence fixtures are owned by their respective workers and may already be stopped; do not rerun them against saved state or reopen old audit threads casually. Copy the pattern into a fresh isolated fixture, retain source/binary identities and a cleanup manifest, and close every thread/process that fixture starts. This matrix launched no application or provider processes.

## Coverage matrix

The complete machine-readable cases, including required recovery and cross-surface checks, are in [workflow-coverage-correction.json](workflow-coverage-correction.json). Existing issue IDs link back to [issues.json](../issues.json); mapping a record does not upgrade its evidence level.

### Navigation

#### NAV-01 — Open the PR the user clicked

**Partial runtime failure.** Priority P1; follow-up owner `pr_entry_runtime_correction`. Existing records: `pr-navigation-01`, `pr-navigation-02`, `pr-navigation-04`, `pr-navigation-05`, `pr-navigation-06`, `link-affordances-07`.

- **Trigger:** Keep thread A focused, hide the right panel, and click the PR badge on thread B; repeat through footer, Overview and context menu.
- **Visible result required:** B’s PR is visible in the intended native destination; the selected thread, PR identity and copied URL agree.
- **Failure/recovery required:** Missing URL, unavailable browser ownership or PR read failure produces a visible explanation on the user’s current surface.
- **Cross-surface checks:** Repeat with another project filter, open Browser and a second PR target; preserve A’s unsent draft.
- **Next runtime check:** Test cross-thread navigation from absent/empty board and remaining entry points; require the clicked target visibly appear while preserving the prior draft.
- **Observed branches:** Production gh discovery supplies A/#101 and B/#202. With A selected and panel hidden, an actual click on B’s badge leaves A selected and no visible panel; an OCR/element assertion fails in repeated runs. A’s own badge opens Browser; Overview’s PR row opens native #101 with populated Summary and Timeline. The split destinations are directly observed.
- **Still unverified:** B absent from board/empty board, footer and external/modifier routes, CEF GitHub load, no-PR/missing-URL/error targeting and preservation of an unrelated draft were not exercised.
- **Correction observations:** `PR report NAV-01`, `PR report NAV-02`.
- **Executed evidence:** [pr-entry-runtime-correction.md](pr-entry-runtime-correction.md), [18-cross-before.png](../evidence/pr-entry-correction/18-cross-before.png), [19-cross-after.png](../evidence/pr-entry-correction/19-cross-after.png), [cross-thread-verdict.json](../evidence/pr-entry-correction/cross-thread-verdict.json), [02-focused-thread-pr-click.png](../evidence/pr-entry-correction/02-focused-thread-pr-click.png), [07-native-summary.png](../evidence/pr-entry-correction/07-native-summary.png), [08-native-timeline.png](../evidence/pr-entry-correction/08-native-timeline.png).

#### NAV-02 — Refresh a PR and recover from failed reads

**Partial runtime failure.** Priority P1; follow-up owner `pr_entry_runtime_correction`. Existing records: `pr-operations-001`, `pr-operations-002`, `pr-operations-003`, `pr-operations-015`, `pr-operations-016`, `quality-correction-004`.

- **Trigger:** Open PR details, change the fixture’s checks/comments, refresh, then inject an error and delayed old response.
- **Visible result required:** The visible current PR refreshes checks and comments together; a late response cannot replace a newer target.
- **Failure/recovery required:** Keep last useful content, show a scoped error, make Retry actually retry, and cancel or bound abandoned reads.
- **Cross-surface checks:** Switch PR/thread and close/reopen while a read is outstanding.
- **Next runtime check:** Verify checks/body/comments always invalidate current detail and preserve visibly stale content during failures; then exercise concurrency/auth and action-driven refresh.
- **Observed branches:** Actual thread-menu Refresh and palette Refresh trigger one lightweight PR read but no detailed read when only checks/body/comments change. Native review remains stale while Overview reflects new CI. Re-entering Overview’s PR row reloads details. A synthetic OPEN→MERGED change does trigger detailed refresh and new body/check/comment rendering. No forge write occurs.
- **Still unverified:** Auth expiry, transient failed reload retaining prior content, out-of-order reads/cancellation/deadline and agent-action refresh remain untested. Initial synthetic-repository read error was clipped in the panel, but the fixture caused the error; native network/auth recovery is not established.
- **Correction observations:** `PR report NAV-03`, `quality-correction-004`.
- **Executed evidence:** [pr-entry-runtime-correction.md](pr-entry-runtime-correction.md), [13-refreshed-stale-timeline.png](../evidence/pr-entry-correction/13-refreshed-stale-timeline.png), [14-refreshed-stale-summary.png](../evidence/pr-entry-correction/14-refreshed-stale-summary.png), [15-overview-fresh-checks.png](../evidence/pr-entry-correction/15-overview-fresh-checks.png), [refresh-verdict.json](../evidence/pr-entry-correction/refresh-verdict.json), [17-reopened-summary-settled.png](../evidence/pr-entry-correction/17-reopened-summary-settled.png), [24-lifecycle-refresh-settled.png](../evidence/pr-entry-correction/24-lifecycle-refresh-settled.png), [25-lifecycle-new-comment.png](../evidence/pr-entry-correction/25-lifecycle-new-comment.png), [04-overview-native-pr.png](../evidence/pr-entry-correction/04-overview-native-pr.png).

#### NAV-03 — Read the complete PR review

**Positive runtime branch only.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `pr-operations-004`, `pr-operations-005`, `pr-operations-006`, `pr-operations-007`, `pr-operations-008`, `pr-operations-009`, `pr-operations-017`, `pr-operations-018`, `pr-operations-019`.

- **Trigger:** Open Summary, Guide, Timeline and Code for a PR containing replies, approvals, resolved threads, rich text and failed checks.
- **Visible result required:** The review is readable, ordered and complete or explicitly truncated; the newest intended guide and the actual check failure are reachable.
- **Failure/recovery required:** Empty, partial, stale and truncated records have an honest explanation and retrieval route.
- **Cross-surface checks:** Compare visible identity and checkout synchronization across tabs and after refresh.
- **Next runtime check:** Populate complex/long review data and exercise Guide/Code/check routes; initial Summary/Timeline rendering is only a positive entry branch.
- **Observed branches:** Overview enters a native PR view populated through production reviewr plus installed gh/local Unix-socket forge fixture. Summary shows description/checks and Timeline shows the controlled comment.
- **Still unverified:** Guide generation, Code/review interaction, full replies/verdicts, long data caps, check-detail navigation, description formatting and sync context remain unverified.
- **Correction observations:** `PR report NAV-02 native detail control`.
- **Executed evidence:** [pr-entry-runtime-correction.md](pr-entry-runtime-correction.md), [07-native-summary.png](../evidence/pr-entry-correction/07-native-summary.png), [08-native-timeline.png](../evidence/pr-entry-correction/08-native-timeline.png).

#### NAV-04 — Run an existing PR action against the correct branch

**Not exercised.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `pr-operations-012`, `pr-operations-014`, `pr-operations-020`, `pr-operations-021`.

- **Trigger:** With real branch changes absent from current transcript edits, invoke existing Create, Fix, Guide or Merge controls in a disposable fixture.
- **Visible result required:** Availability reflects repository state; the prompt names the actual forge/repository/PR; pending and completion are visible.
- **Failure/recovery required:** Failure leaves recoverable context and a working retry; unavailable merge methods are explained.
- **Cross-surface checks:** Queue an action while changing focused thread and ensure its target does not change.
- **Next runtime check:** Capture generated prompts in the fake provider only; never perform a real create/merge/push.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### NAV-05 — Search, select and open the intended thread

**Partial runtime failure.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `projects-navigation-005`, `projects-navigation-006`, `projects-navigation-007`, `projects-navigation-009`.

- **Trigger:** Open sidebar search with many results, navigate below the fold, allow result reordering, then press Enter.
- **Visible result required:** The highlighted result remains visible and stable; Enter opens that exact thread and its selection remains discoverable.
- **Failure/recovery required:** No matches, disappearing matches and filtered-out destinations explain what happened without losing the query.
- **Cross-surface checks:** Switch project filter while searching; preserve draft and return to same result after closing search.
- **Next runtime check:** Keep highlighted search result in view when query/cursor changes; then exercise keyboard moves beyond viewport, live reordering/disappearance and project-filter context.
- **Observed branches:** A broad 44-thread search retains prior sidebar scroll offset while resetting cursor to result 0: visible rows begin at 09, selected result 02 is above viewport, and Enter opens 02. In a separate 14-result control, twelve Down keys highlight a visible result 14 and Enter opens that exact thread.
- **Still unverified:** Arrow selection moving beyond viewport, live result reorder/disappearance, title-only search scope, project-filter target visibility and retained drafts remain untested.
- **Correction observations:** `NAV05-search-scroll-cursor`.
- **Executed evidence:** [navigation-runtime-correction.md](navigation-runtime-correction.md), [13-search-44-results.png](../evidence/navigation-correction/13-search-44-results.png), [14-search-offscreen-zero.png](../evidence/navigation-correction/14-search-offscreen-zero.png), [15-enter-offscreen-zero-opens-02.png](../evidence/navigation-correction/15-enter-offscreen-zero-opens-02.png), [11-search-down-12.png](../evidence/navigation-correction/11-search-down-12.png), [12-search-enter-12.png](../evidence/navigation-correction/12-search-enter-12.png).

#### NAV-06 — Rename, park, undo and reopen a thread

**Partial runtime failure.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `projects-navigation-001`, `projects-navigation-002`, `projects-navigation-003`, `projects-navigation-004`, `projects-navigation-011`, `projects-navigation-014`.

- **Trigger:** Rename working, snoozed and parked rows; park one grouped thread and a multi-selection; undo and reopen.
- **Visible result required:** A visible editor owns typing; undo restores identity and grouping; reopen shows history and the expected active thread.
- **Failure/recovery required:** Failed park/reopen remains visible per item, retains selection and permits retry.
- **Cross-surface checks:** Check groups, project filters, search and existing drafts after each action.
- **Next runtime check:** Exercise a visible Parked row and establish actual rename input/save/cancel on Working/settled rows; then park/undo/reopen failure and group restoration.
- **Observed branches:** Snoozed slim row offers Rename, but selecting it draws no rename hint/editor and stored title stays unchanged. Working and settled row actions display a rename hint. For Working, the specific rename-thread-2 selector was absent; type/Enter acknowledged input but the full editor text/commit was not established. An immediate title read had no settle interval or later persistence check. This is inconclusive, not a saved-rename failure. Settled typing/save was not completed.
- **Still unverified:** Parked row Rename was not clicked because the target was outside the drawn slice. Working input/commit and settled rename save remain unresolved. Normal rename Cancel, failed reopen, grouped Undo Park, bulk failure and Cmd+W remain unverified.
- **Correction observations:** `NAV06-rename-snoozed`, `NAV06-rename-working inconclusive`, `NAV06-rename-settled entry only`.
- **Executed evidence:** [navigation-runtime-correction.md](navigation-runtime-correction.md), [17-working-rename-editor.png](../evidence/navigation-correction/17-working-rename-editor.png), [19-working-rename-saved.png](../evidence/navigation-correction/19-working-rename-saved.png), [22-settled-row-rename.png](../evidence/navigation-correction/22-settled-row-rename.png), [26-snoozed-row-menu.png](../evidence/navigation-correction/26-snoozed-row-menu.png), [27-snoozed-rename-attempt.png](../evidence/navigation-correction/27-snoozed-rename-attempt.png).

#### NAV-07 — Keep project and checkout context across new work

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `projects-navigation-008`, `projects-navigation-012`, `projects-navigation-013`.

- **Trigger:** Scope to a project, create a thread from the current branch/worktree, edit the project, and return.
- **Visible result required:** Visible project, checkout and new-thread defaults follow the chosen context without hidden re-selection.
- **Failure/recovery required:** Invalid or unavailable paths have a clear recovery route.
- **Cross-surface checks:** Restart and search from scoped state; open an external thread link.
- **Next runtime check:** Use two scratch repositories and a worktree, checking actual cwd in provider launch records.
- **Still unverified:** The navigation fixture did not exercise draft preservation, project-filter switching or New thread checkout retargeting.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Shell

#### SHELL-01 — Open and close sidebars with coherent motion

**Not exercised.** Priority P1; follow-up owner `parent native inspection`. Existing records: `motion-001`, `motion-003`, `motion-004`, `motion-005`, `motion-009`, `motion-011`.

- **Trigger:** Toggle left sidebar, right panel, tab bodies, menus, shelves, Find and notices through their ordinary controls.
- **Visible result required:** Each structural change preserves context with the required interruptible opening/closing behavior; content does not snap unexpectedly.
- **Failure/recovery required:** Reverse a transition mid-flight; reduced motion yields a deliberate stable final state.
- **Cross-surface checks:** Keep reading a long response while switching tools and toggling panels.
- **Next runtime check:** Native intermediate-frame timing remains required; visual-quality source review confirms snap policy but settled endpoint captures do not establish motion.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### SHELL-02 — Restore sensible panel width after using a wide tool

**Partial runtime failure.** Priority P1; follow-up owner `panel_runtime_correction`. Existing records: `panel-tools-018`, `input-focus-008`.

- **Trigger:** Open Overview, open Files/Changes, return to Overview, close/reopen, resize, maximize/restore, and restart.
- **Visible result required:** The previous operator-selected size and reading space are respected; a tool’s minimum does not silently become the global saved width.
- **Failure/recovery required:** Narrow windows remain usable and dragging continues across child content.
- **Cross-surface checks:** Check left navigation visibility, conversation width and reading position at every step.
- **Next runtime check:** Two fresh threads and app restart confirm sticky width at 1440 × 900; continue actual divider drag, narrow-window, and operator-chosen-width restore checks in native runtime.
- **Observed branches:** Overview 320 → Files 864 → Overview 864 at 1440 px, without operator resize; wide state persists through close/reopen and app restart. An independent fresh thread reproduces the same growth via Changes; nav hides and conversation contracts. Visual review independently confirms sparse Overview and Browser inheriting the broad allocation.
- **Still unverified:** Native narrow-window behavior, actual divider dragging, restored operator-selected width and intermediate motion remain untested.
- **Correction observations:** `panel-tools-018 bounded sticky-width branch`, `VQ-C04`.
- **Executed evidence:** [overview-width-contract.json](../evidence/panel-correction/overview-width-contract.json), [03-overview-reopened.png](../evidence/panel-correction/03-overview-reopened.png), [05-overview-after-files.png](../evidence/panel-correction/05-overview-after-files.png), [07-reopened-after-files.png](../evidence/panel-correction/07-reopened-after-files.png), [panel-runtime-correction.md](panel-runtime-correction.md), [independent-width-contract.json](../evidence/panel-correction/independent-width-contract.json), [21-after-headless-restart.png](../evidence/panel-correction/21-after-headless-restart.png), [25-independent-overview-reopened.png](../evidence/panel-correction/25-independent-overview-reopened.png), [visual-quality-correction.md](visual-quality-correction.md).

#### SHELL-03 — Keep keys in the control the user clicked

**Partial runtime failure.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `input-focus-001`, `input-focus-003`, `input-focus-005`, `input-focus-009`, `input-focus-010`.

- **Trigger:** Leave a sentinel draft, click file editor/filter, launcher, browser form or another panel control, and type/navigation-submit keys.
- **Visible result required:** Typing and actions belong to the intended visible control; the conversation sends only on explicit composer submission.
- **Failure/recovery required:** Redraw, provider updates, failed actions and closed menus restore focus to a valid control.
- **Cross-surface checks:** Cross Files, Browser, terminal, side chat, PR tabs, palette and composer.
- **Next runtime check:** Retain reproduced Files and empty-launcher checks; add native browser form Tab/Enter/Escape/paste/IME and accessibility traversal.
- **Observed branches:** Form parent-wrapper click/type attempts reached Main, but exact nested Input hits were not established; this is an inconclusive addition, not another certified routing defect.
- **Still unverified:** Native Browser input, valid form field traversal, accessibility focus and unrelated surface routes remain open. Resolve form nested-input bounds and headless paint ambiguity before claiming another production focus failure.
- **Executed evidence:** [runtime-interactions.md](runtime-interactions.md), [04-editor-next-keys.png](../evidence/headless/04-editor-next-keys.png), [06-filter-next-keys.png](../evidence/headless/06-filter-next-keys.png), [10-panel-after-enter.png](../evidence/headless/10-panel-after-enter.png), [forms-runtime-correction.md](forms-runtime-correction.md), [04-after-form-submit.png](../evidence/forms-correction/04-after-form-submit.png), [08-coordinate-click.png](../evidence/forms-correction/08-coordinate-click.png).

#### SHELL-04 — Close or dismiss the surface the user is operating

**Partial runtime failure.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `input-focus-004`, `link-affordances-06`.

- **Trigger:** With a PR/terminal/side-chat/file panel or context menu focused, press Cmd+W or Escape.
- **Visible result required:** Close removes the intended tool; Escape dismisses the top interaction; the parent thread and draft survive.
- **Failure/recovery required:** Reopen restores the same resource and focus.
- **Cross-surface checks:** Repeat with nested menus, Find, image preview, browser content and streaming main thread.
- **Next runtime check:** Test menu Escape (already failing) plus Cmd+W on disposable panel sessions and Reopen closed surface.
- **Executed evidence:** [30-web-menu-after-escape.png](../evidence/headless/30-web-menu-after-escape.png).

### Live output

#### LIVE-01 — Read live main and tool output as it arrives

**Positive runtime branch only.** Priority P1; follow-up owner `agent_live_runtime_correction`. Existing records: `motion-008`, `transcript-004`, `scroll-quality-001`, `scroll-quality-003`.

- **Trigger:** Start a controlled long response with intermittent tool output; view it before completion, navigate away and return.
- **Visible result required:** New text and tool output visibly arrive on the correct thread without waiting for final completion; follow behavior is understandable.
- **Failure/recovery required:** Pause, error, stop and provider disconnect retain what arrived and show the true state.
- **Cross-surface checks:** Read earlier content, open Files/browser, switch threads and return during streaming.
- **Next runtime check:** Continue valid long/lifecycle provider replay and live navigation checks; do not turn the observed incremental visibility into a pass for scrolling or every provider.
- **Observed branches:** Main MAIN_LIVE_ONE and MAIN_LIVE_TWO appeared before completion through the controlled Claude protocol-to-renderer path. Open Alpha received second and third forwarded child assistant messages without reopen or refresh. A separate controlled Read fixture rendered APPEND_01 through APPEND_08 and END_SENTINEL in order.
- **Still unverified:** Long output, intermittent tool output, provider disconnect, stop, cross-thread return and manual-scroll interactions remain untested by this run. Child token deltas are outside the tested Claude adapter contract. Synthetic final Main snapshot duplicated a sentence; invalid fixture lifecycle is excluded from product defects. Append was released after the confounded blank paging state, so no known manually selected reading position was established for preservation testing.
- **Correction observations:** `agent-live working Main/child updates`.
- **Executed evidence:** [agent-live-correction.md](agent-live-correction.md), [02-main-first-live-clear.png](../evidence/agent-live-correction/02-main-first-live-clear.png), [03-main-second-live.png](../evidence/agent-live-correction/03-main-second-live.png), [10-agent-open-live-three.png](../evidence/agent-live-correction/10-agent-open-live-three.png), [scroll-runtime-correction.md](scroll-runtime-correction.md), [12-after-append.png](../evidence/scroll-correction/12-after-append.png), [provider-emitted.jsonl](../evidence/scroll-correction/provider-emitted.jsonl).

### Agents

#### AGENT-01 — Open a child and follow nested live work

**Partial runtime failure.** Priority P1; follow-up owner `agent_live_runtime_correction`. Existing records: `subagents-background-02`, `subagents-background-04`, `quality-correction-001`.

- **Trigger:** From a rendered spawn row open a running child, inspect its incrementally arriving output, then open its child and return to parent/root.
- **Visible result required:** The named child’s real output, identity and status appear; nested relationships and parent/root navigation remain available.
- **Failure/recovery required:** Empty, waiting, failed and unavailable children are distinct states with a useful action.
- **Cross-surface checks:** Keep main composer/draft and selected child stable while unrelated children update.
- **Next runtime check:** Reproduce completed and failed nested routes, then verify ordinary prose across actual resize/maximize/restore; preserve event-boundary evidence.
- **Observed branches:** Main row opens the correct Alpha child; its open transcript receives supported forwarded messages and completion. Inside Alpha, the Grandchild row and its View live output text are both inert despite emitted child output. At ordinary 420px width, child prose loses sentence endings; maximizing to 960 px reveals the complete text.
- **Still unverified:** Immediate-parent/root navigation, completed nested opening, multiple unrelated children, failures and native resizing remain untested. Child prose clipping is a distinct correction finding; the layout root cause has not been isolated.
- **Correction observations:** `agent-live-05`, `agent-live-07`, `quality-correction-001`.
- **Executed evidence:** [agent-live-correction.md](agent-live-correction.md), [06-agent-open-live-two-nested.png](../evidence/agent-live-correction/06-agent-open-live-two-nested.png), [07-nested-row-click.png](../evidence/agent-live-correction/07-nested-row-click.png), [08-nested-live-output-click.png](../evidence/agent-live-correction/08-nested-live-output-click.png), [13-completed-agent.png](../evidence/agent-live-correction/13-completed-agent.png), [14-agent-maximized-output.png](../evidence/agent-live-correction/14-agent-maximized-output.png).

#### AGENT-02 — Handle a child decision and stop the intended work

**Not exercised.** Priority P1; follow-up owner `agent_live_runtime_correction`. Existing records: `subagents-background-04`, `subagents-background-05`, `subagents-background-06`, `subagents-background-07`.

- **Trigger:** Receive a child decision, inspect it, answer it, then stop a mixed child/command task.
- **Visible result required:** The request and control name their target; the reply/stop reaches only that target; opening Main stays in Main.
- **Failure/recovery required:** Rejected or ineffective Stop clears pending state, explains failure and permits retry.
- **Cross-surface checks:** Verify transcript summary, Background panel, agent tab and parent status agree.
- **Next runtime check:** Decision and Stop were not part of the completed live-agent fixture. Use a controlled decision/stop-capable protocol fixture and capture outgoing target IDs and UI states.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### AGENT-03 — Close, reopen and restore agent history

**Partial runtime failure.** Priority P1; follow-up owner `agent_live_runtime_correction`. Existing records: `subagents-background-01`, `subagents-background-03`, `subagents-background-09`.

- **Trigger:** Open an agent with output, close its tab, reopen it, then restart with that tab selected and inspect an evicted child.
- **Visible result required:** Reopening restores content, not an empty tab; restart preserves target or clearly recovers it; stored history loads when needed.
- **Failure/recovery required:** History loading, failure and retry are visible and do not resume the provider unexpectedly.
- **Cross-surface checks:** Repeat SideChat separately because it has different backing resources; compare Files which already has a hydration path.
- **Next runtime check:** Single-tab Agent and SideChat reopen failures are confirmed separately. Continue Close all, restart, eviction and completed SideChat history; Cmd+W targeting is still open.
- **Observed branches:** Closing Alpha and choosing Reopen closed surface through the palette restores its selected tab with a blank body. agent-drill is absent while panel-tab-Agent exists; clicking the original Main row reconstructs the child content. Separately, selecting real transcript text and clicking selection-side-chat opens a working SideChat with a child thread and typeable composer. Closing that SideChat removes its child; palette Reopen closed surface restores only the selected tab. Body, header and composer remain absent after settled recapture and explicit activation.
- **Still unverified:** Close all, restart, eviction hydration, failed history load/retry and provider launch behavior across those branches remain untested. SideChat prompt submission, completed reply restoration, real-provider fork continuity and Cmd+W were not exercised.
- **Correction observations:** `agent-live-06`, `draft-lifecycle-runtime-03`.
- **Executed evidence:** [agent-live-correction.md](agent-live-correction.md), [11-reopen-palette.png](../evidence/agent-live-correction/11-reopen-palette.png), [12-reopened-agent-blank.png](../evidence/agent-live-correction/12-reopened-agent-blank.png), [13-completed-agent.png](../evidence/agent-live-correction/13-completed-agent.png), [draft-lifecycle-runtime-correction.md](draft-lifecycle-runtime-correction.md), [13-sidechat-open.png](../evidence/draft-lifecycle-correction/13-sidechat-open.png), [15-sidechat-draft-confirmed.png](../evidence/draft-lifecycle-correction/15-sidechat-draft-confirmed.png), [19-sidechat-reopened-settled.png](../evidence/draft-lifecycle-correction/19-sidechat-reopened-settled.png).

#### AGENT-04 — Find all running work consistently

**Partial runtime failure.** Priority P1; follow-up owner `agent_live_runtime_correction`. Existing records: `subagents-background-08`.

- **Trigger:** Run a child and a shell task concurrently; open transcript summary, Background and agent inspection.
- **Visible result required:** All views agree on which work exists, its owner and whether the main turn is awaiting it.
- **Failure/recovery required:** Completion/failure removes or settles the correct row without stale Stopping labels.
- **Cross-surface checks:** Switch threads with reused task IDs and return.
- **Next runtime check:** Keep the proven wrong-output-route branch and continue mixed shell/child inventory and lifecycle comparison; this is not evidence that child updates never arrive.
- **Observed branches:** After Alpha output is visible in its child transcript, Main row View live output opens Background and says No output yet for the same child.
- **Still unverified:** Concurrent shell/child inventory, reused IDs, nested Background inclusion, stop state, failure/completion settlement and thread switching remain untested.
- **Correction observations:** `agent-live-04`.
- **Executed evidence:** [agent-live-correction.md](agent-live-correction.md), [06-agent-open-live-two-nested.png](../evidence/agent-live-correction/06-agent-open-live-two-nested.png), [09-main-live-output-click.png](../evidence/agent-live-correction/09-main-live-output-click.png).

### Composer

#### COMPOSE-01 — Send without losing a refused draft

**Not exercised.** Priority P1; follow-up owner `draft_lifecycle_runtime_correction`. Existing records: `composer-01`.

- **Trigger:** Compose text, quote and attachment, then submit during a definite admission refusal.
- **Visible result required:** The full prompt remains editable or clearly recoverable until admitted; accepted content appears once.
- **Failure/recovery required:** Retry preserves context without overwriting newer typing or duplicating an uncertain send.
- **Cross-surface checks:** Switch threads or receive provider startup updates while sending.
- **Next runtime check:** Use deterministic provider-start/admission failure; capture composer before/after and fake-provider receipts.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### COMPOSE-02 — Retain a never-sent draft across navigation and restart

**Partial runtime failure.** Priority P1; follow-up owner `draft_lifecycle_runtime_correction`. Existing records: `composer-02`.

- **Trigger:** Create a new unsent pane, type a sentinel and context, wait for autosave, navigate away and restart the owned window.
- **Visible result required:** The same unsent draft and context return, just as an existing thread draft does.
- **Failure/recovery required:** Provider unavailable or no checkout still preserves authored text.
- **Cross-surface checks:** Compare new unsent pane, existing thread, Park/reopen and sidebar filter transitions.
- **Next runtime check:** Exercise normal Quit and recovery of launch context, multiple new drafts and first-send promotion; the proved boundary is window process termination after autosave.
- **Observed branches:** A new Cmd+N pane visibly contained its plain-text sentinel after autosave time, but drafts.json held only the existing-thread control. After only the owned window process was terminated and restarted, the existing-thread control restored and a new pane was blank.
- **Still unverified:** Normal Quit, multiple new drafts, attachments, model/checkout recovery, first-send promotion and draft navigation permutations remain untested.
- **Executed evidence:** [draft-lifecycle-runtime-correction.md](draft-lifecycle-runtime-correction.md), [03-unsent-draft-before.png](../evidence/draft-lifecycle-correction/03-unsent-draft-before.png), [04-after-window-restart.png](../evidence/draft-lifecycle-correction/04-after-window-restart.png), [06-new-draft-after-restart.png](../evidence/draft-lifecycle-correction/06-new-draft-after-restart.png), [drafts-before-restart.json](../evidence/draft-lifecycle-correction/drafts-before-restart.json).

#### COMPOSE-03 — Edit a queued prompt without merging unrelated work

**Partial runtime failure.** Priority P1; follow-up owner `draft_lifecycle_runtime_correction`. Existing records: `composer-04`.

- **Trigger:** During a controlled busy turn queue a prompt, type a different unsent draft, and click queued Edit.
- **Visible result required:** The queued item remains independently editable; its text and attachments do not merge silently with the unrelated draft.
- **Failure/recovery required:** Cancel/edit failure keeps both original items recoverable and preserves order.
- **Cross-surface checks:** Repeat with native queue acknowledgements arriving during edits and with a second queued item.
- **Next runtime check:** Verify distinct identities through multi-item Edit, cancellation races and resubmission; preserve unrelated composer text and provider acknowledgements.
- **Observed branches:** One native queued prompt and a separate unsent draft existed before the actual queued-edit-0 click. The click sent cancel_async_message for that exact UUID, queue count fell from 1 to 0, and persisted composer text became QUEUED_SENTINEL followed by UNRELATED_DRAFT. Both texts survive, but separate message identities are merged.
- **Still unverified:** Multiple queued items/order, resubmission, provider-consumption race and a committed Save/Cancel editing flow remain untested; no Save/Cancel flow was presented. The earlier fixture lacked queue capability and was correctly refused with text retained; that setup failure is excluded from defects.
- **Executed evidence:** [draft-lifecycle-runtime-correction.md](draft-lifecycle-runtime-correction.md), [08-queue-before-edit-valid.png](../evidence/draft-lifecycle-correction/08-queue-before-edit-valid.png), [09-queue-after-edit.png](../evidence/draft-lifecycle-correction/09-queue-after-edit.png), [queue-before-edit.json](../evidence/draft-lifecycle-correction/queue-before-edit.json), [queue-after-edit.json](../evidence/draft-lifecycle-correction/queue-after-edit.json), [drafts-after-queue-edit.json](../evidence/draft-lifecycle-correction/drafts-after-queue-edit.json).

#### COMPOSE-04 — Understand Stop and queued continuation

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `composer-06`.

- **Trigger:** Queue a second prompt, then press the visible Stop control while the first turn runs.
- **Visible result required:** The result clearly states whether only the current turn or all pending work stopped; the observed behavior matches the UI language.
- **Failure/recovery required:** Stop failure and queued cancellation have a visible retry path.
- **Cross-surface checks:** Compare main turn, child work and queued-native versus locally queued prompts.
- **Next runtime check:** Test existing Stop semantics with a deterministic queue. This is a product-contract assessment, not an automatic failure of a deliberately chosen policy.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Quotes

#### QUOTE-01 — Select, cite, add general text and send

**Positive runtime branch only.** Priority P1; follow-up owner `attachments_runtime_correction`. Existing records: `quote-attachments-003`, `quote-attachments-012`.

- **Trigger:** Select a passage by double-click, drag and keyboard; invoke Cite, type ordinary text, remove/undo and send.
- **Visible result required:** The intended exact selection becomes one usable quote and ordinary text can be added immediately; send carries both.
- **Failure/recovery required:** Selection changes, streaming and long/complex passages do not silently lose the action or text.
- **Cross-surface checks:** Repeat in code, multiline prose, tables, folded content and narrow layouts.
- **Next runtime check:** Use native multiline, code and streaming selection paths; retain distinct short Cite and 990-character whole-context successes.
- **Observed branches:** Prior short double-click selection Cite plus general commentary/send remains a working branch. A separate whole-response Attach message as context route sends 991 source characters (990 trimmed), including prose/Rust and separate general commentary, intact in the provider frame.
- **Still unverified:** Long drag/multiline/code/keyboard/streaming selection-Cite remains untested; whole-response context attachment is a different entry path.
- **Executed evidence:** [21-clean-cite-and-annotation.png](../evidence/headless/21-clean-cite-and-annotation.png), [23-quote-sent.png](../evidence/headless/23-quote-sent.png), [provider-host.jsonl](../evidence/headless/provider-host.jsonl), [attachment-runtime-correction.md](attachment-runtime-correction.md), [13-whole-response-cited.png](../evidence/attachment-correction/13-whole-response-cited.png), [16-long-cite-with-commentary.png](../evidence/attachment-correction/16-long-cite-with-commentary.png), [17-long-cite-sent.png](../evidence/attachment-correction/17-long-cite-sent.png), [provider-host.jsonl](../evidence/attachment-correction/provider-host.jsonl).

#### QUOTE-02 — Inspect and comment on individual quoted passages

**Partial runtime failure.** Priority P1; follow-up owner `attachments_runtime_correction`. Existing records: `quote-attachments-001`, `quote-attachments-002`, `transcript-006`.

- **Trigger:** Cite two longer passages, inspect each in full, add a distinct per-quote comment and navigate to its source.
- **Visible result required:** Quoted prose is readable and inspectable; each comment remains paired with its passage/source through send and reopen.
- **Failure/recovery required:** Cancel/edit does not discard the quote or the general composer text.
- **Cross-surface checks:** Compare draft shelf, sent transcript, retry and reopened draft.
- **Next runtime check:** Verify readable full-source inspection, per-quote comments and source navigation for long/multiple quotes without changing general commentary behavior.
- **Observed branches:** The 990-trimmed-character response becomes one 88×88 tile with only a few opening words and ellipsis. Clicking the tile reveals no full text or per-quote comment editor; q is unhandled. After send, the Quote row also clips after two lines. Complete provider payload is retained, so this is inspection/editability failure, not payload loss.
- **Still unverified:** Other quote counts, keyboard traversal, source navigation, selected-code contexts and source-deleted states remain unverified.
- **Executed evidence:** [21-clean-cite-and-annotation.png](../evidence/headless/21-clean-cite-and-annotation.png), [22-clean-tile-no-editor.png](../evidence/headless/22-clean-tile-no-editor.png), [attachment-runtime-correction.md](attachment-runtime-correction.md), [13-whole-response-cited.png](../evidence/attachment-correction/13-whole-response-cited.png), [14-tile-click.png](../evidence/attachment-correction/14-tile-click.png), [15-tile-key-no-editor.png](../evidence/attachment-correction/15-tile-key-no-editor.png), [17-long-cite-sent.png](../evidence/attachment-correction/17-long-cite-sent.png).

#### QUOTE-03 — Keep quote formatting and editability through attachments and restore

**Positive runtime branch only.** Priority P2; follow-up owner `attachments_runtime_correction`. Existing records: `quote-attachments-004`, `quote-attachments-005`, `quote-attachments-006`, `composer-09`.

- **Trigger:** Compose a literal blockquote, Cite a passage, attach a file, send/retry/reopen; also attach a long whole message.
- **Visible result required:** Literal text stays literal, quotes keep their presentation and identity, and file previews remain visible.
- **Failure/recovery required:** Oversized or unsupported context gives an actionable explanation rather than a silent no-op.
- **Cross-surface checks:** Compare draft encoding, sent transcript rendering, history recall and queued editing.
- **Next runtime check:** Test the actual 10,000/10,001 boundary, literal leading general-editor blockquotes, queue recall and files; do not clear the mapped source findings from this success.
- **Observed branches:** The 990-trimmed-character whole-context passage and ordinary commentary survive a full isolated server/window restart and then send intact. A literal blockquote inside deliberately attached source stays in the same one tile after restore; its outgoing Markdown uses nested > >.
- **Still unverified:** 10,000/10,001 whole-context character boundaries in quote-attachments-004 are not tested by this 990-character fixture. Literal leading Markdown typed in the general editor (quote-attachments-005) is distinct from blockquote text nested inside intentional context and remains untested. Retry, queue recall, multiple quote identities, file-attached quote rendering and downstream provider interpretation remain unverified.
- **Correction observations:** `QUOTE-03 bounded context restore`.
- **Executed evidence:** [attachment-runtime-correction.md](attachment-runtime-correction.md), [19-draft-before-restart.png](../evidence/attachment-correction/19-draft-before-restart.png), [20-after-server-restart.png](../evidence/attachment-correction/20-after-server-restart.png), [21-restored-draft-sent.png](../evidence/attachment-correction/21-restored-draft-sent.png), [provider-host.jsonl](../evidence/attachment-correction/provider-host.jsonl).

#### QUOTE-04 — Keep cited reference text inert

**Not exercised.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `quote-attachments-007`.

- **Trigger:** Cite text containing @file and skill syntax, add ordinary instruction and send.
- **Visible result required:** The outgoing provider prompt treats the passage as quoted reference; no unintended file or skill input is activated.
- **Failure/recovery required:** Retry/restoration preserves that distinction.
- **Cross-surface checks:** Compare deliberate @file attachment with the same syntax only inside a quote.
- **Next runtime check:** Capture both adapters’ actual outgoing typed input using fixture files and fake providers.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Attachments

#### ATTACH-01 — Attach, inspect, remove and send supported files/images

**Not exercised.** Priority P1; follow-up owner `attachments_runtime_correction`. Existing records: `composer-07`, `quote-attachments-008`, `quote-attachments-009`, `quote-attachments-011`.

- **Trigger:** Attach PNG plus supported SVG/AVIF/TIFF, open previews, navigate by keyboard, remove one, and add many at narrow width.
- **Visible result required:** Previews and readiness states are truthful, inspectable and bounded; unsupported input has a useful fallback.
- **Failure/recovery required:** A repaired/newly created file can retry decoding without restarting; loading/failure preserves user context.
- **Cross-surface checks:** Repeat before send, after send, during typing, with panel open and after redraw.
- **Next runtime check:** Use a native owned picker/drop fixture for actual files and supported image formats, then preview/remove/send/history; no headless intake pass.
- **Observed branches:** Add files or photos is visible, but the native chooser was outside documented headless controls and no file or image was attached.
- **Executed evidence:** [attachment-runtime-correction.md](attachment-runtime-correction.md), [22-attachment-add-menu.png](../evidence/attachment-correction/22-attachment-add-menu.png), [23-file-picker.png](../evidence/attachment-correction/23-file-picker.png).

#### ATTACH-02 — Preserve attachment meaning across remote send and history

**Not exercised.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `composer-03`, `quote-attachments-010`.

- **Trigger:** Attach a local file to a remote-backed thread, send, then change/delete the original and inspect retry/history.
- **Visible result required:** The target receives the intended content or a clear transfer limitation; history states whether it refers to mutable path or retained content.
- **Failure/recovery required:** Missing or changed paths have explicit recovery rather than a misleading identical preview.
- **Cross-surface checks:** Compare local/remote machines, pasted screenshots and ordinary paths.
- **Next runtime check:** Needs isolated remote fixture; inspect actual provider bytes and paths. Mutable-path history is a declared product choice, not automatically a runtime bug.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Composer

#### COMPOSE-05 — Reopen a structured review draft

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `composer-11`.

- **Trigger:** Create a review-comment draft with target metadata, leave and reopen it, then edit/send.
- **Visible result required:** The target, line selection and structured controls remain visible and editable.
- **Failure/recovery required:** Failed send retains the same review context.
- **Cross-surface checks:** Compare normal draft, queue edit, restart and source-file changes.
- **Next runtime check:** Use a disposable review fixture and capture UI plus outgoing review prompt before/after reopen.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Transcript

#### READ-01 — Find text in loaded but folded history

**Partial runtime failure.** Priority P1; follow-up owner `scroll_runtime_correction`. Existing records: `transcript-001`, `scroll-quality-006`.

- **Trigger:** Search for a sentinel inside a folded turn/tool output, including during live follow.
- **Visible result required:** Find counts the actual loaded text, opens/reveals the match and keeps it visible.
- **Failure/recovery required:** No matches is accurate; Escape restores focus without interrupting work.
- **Cross-surface checks:** Repeat with a tall visible row, existing jump animation and a right-panel toggle.
- **Next runtime check:** Verify Find across multiple hidden-content kinds and reveal active matches in tall rows without losing manual position.
- **Observed branches:** Find for a known phrase in the loaded but folded Read output reported no matches. Opening the call made its last-passage query return 1/1, while its tail was still folded behind +167 lines. Find for END_SENTINEL in the latest completed answer showed a visible highlighted match.
- **Still unverified:** Other output types, hidden intermediate turns, long-row active-match visibility, changing generations and Find interacting with an existing ease/live follow remain untested.
- **Correction observations:** `SCROLL-RUNTIME-02`, `SCROLL-RUNTIME-03`.
- **Executed evidence:** [scroll-runtime-correction.md](scroll-runtime-correction.md), [05-folded-find.png](../evidence/scroll-correction/05-folded-find.png), [07-expanded-find-last.png](../evidence/scroll-correction/07-expanded-find-last.png), [18-find-sentinel.png](../evidence/scroll-correction/18-find-sentinel.png).

#### READ-02 — Manual navigation wins over live follow and automatic jumps

**Not exercised.** Priority P1; follow-up owner `scroll_runtime_correction`. Existing records: `scroll-quality-001`, `scroll-quality-002`, `scroll-quality-007`, `scroll-quality-008`, `motion-008`, `motion-009`.

- **Trigger:** While output streams or a jump is in progress, use scrollbar track/thumb, wheel, then change panel width or disclosure state.
- **Visible result required:** The chosen reading position remains stable until an explicit return-to-latest action.
- **Failure/recovery required:** Missing/replaced targets cancel cleanly instead of landing on another row.
- **Cross-surface checks:** Use long answers, unmeasured rows, history replacement, small windows and live tool completion.
- **Next runtime check:** Native drag/wheel/resize recording required; public headless driver lacks those events. Existing12send-scroll tests do not cover these transitions.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### READ-03 — Return to latest and page without hiding text

**Runtime inconclusive.** Priority P2; follow-up owner `scroll_runtime_correction`. Existing records: `scroll-quality-003`, `scroll-quality-005`, `scroll-quality-009`.

- **Trigger:** Leave newest content below the viewport, compare latest button and End, then page through a pinned prompt.
- **Visible result required:** Latest cue appears whenever real content is hidden; End and button land consistently without synthetic blank space; page overlap covers every line.
- **Failure/recovery required:** Late layout/runway updates cannot move away from the requested destination.
- **Cross-surface checks:** Repeat after send, while streaming and with pinned prompt height changing.
- **Next runtime check:** Resolve repaint confound with independent geometry/state and native capture before promoting paging; do not assign the observed blank image to an original scroll mechanism.
- **Observed branches:** After PageUp/PageDown, saved headless screenshots showed a blank transcript; End restored content in later captures. Parent inspection also found unrelated sidebar/title/composer chrome missing in the blank captures, so repaint failure confounds a product paging defect.
- **Still unverified:** Independent semantic scroll offsets/visible row evidence or native reproduction is required before classifying blank paging as a product defect. Pinned-area paging coverage, End runway behavior, Latest button accounting, wheel/resize/thumb drag and native macOS routing remain unverified.
- **Correction observations:** `SCROLL-RUNTIME-01 pending repaint investigation`.
- **Executed evidence:** [scroll-runtime-correction.md](scroll-runtime-correction.md), [08-end.png](../evidence/scroll-correction/08-end.png), [09-pageup.png](../evidence/scroll-correction/09-pageup.png), [10-pagedown.png](../evidence/scroll-correction/10-pagedown.png), [11-end-return.png](../evidence/scroll-correction/11-end-return.png).

#### READ-04 — Expand one result without destabilizing surrounding content

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `transcript-004`, `scroll-quality-004`, `motion-005`.

- **Trigger:** At the tail and mid-history, expand a completed tool or one row in a large work group.
- **Visible result required:** The clicked heading stays recognizable and the requested result opens within a bounded layout.
- **Failure/recovery required:** Collapse restores position and focus without a large jump.
- **Cross-surface checks:** Repeat with simultaneous stream append, panel width change and Find result.
- **Next runtime check:** Use a long grouped-tool fixture and click rendered disclosure; compare heading coordinates and total group extent before/after.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### READ-05 — Review the plan and return to a marked answer

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `transcript-003`, `transcript-005`.

- **Trigger:** After a successful turn, expand its work and inspect its plan; pin an answer, navigate away and return/restart.
- **Visible result required:** Completed plan evidence remains recoverable and the marked answer has a discoverable return path matching the action’s label.
- **Failure/recovery required:** Unavailable older material is stated rather than silently missing.
- **Cross-surface checks:** Compare completed/failed/paused turn and reconstructed transcript view.
- **Next runtime check:** Emit real plan/update events and completed result; inspect through rendered entry paths and restart.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Files

#### FILES-01 — Edit and save the file the user opened

**Partial runtime failure.** Priority P1; follow-up owner `files_runtime_correction`. Existing records: `input-focus-010`, `input-focus-009`.

- **Trigger:** Open sample.rs from tree or source link, click text, type multiple keys, Find, and save.
- **Visible result required:** File editor receives input, Find searches the file, save changes disk, and the main draft is untouched.
- **Failure/recovery required:** Read-only/error states remain visible and editable text remains recoverable.
- **Cross-surface checks:** Type across background redraws, switch tabs and return.
- **Next runtime check:** Fix/establish public editor input, then dirty-buffer save/reload/conflict handling. Keep the clean-preview stale-on-revisit branch distinct from destructive overwrite.
- **Observed branches:** Earlier actual editor/filter typing goes to Main. Separately, a clean README preview retains old text after an external disk replacement and selecting the already-open path; closing its inner tab and reopening reads the new sentinel.
- **Still unverified:** Dirty editor/save, conflict safety, concurrent saves, requested source line and native input remain untested.
- **Executed evidence:** [04-editor-next-keys.png](../evidence/headless/04-editor-next-keys.png), [06-filter-next-keys.png](../evidence/headless/06-filter-next-keys.png), [07-file-find.png](../evidence/headless/07-file-find.png), [files-runtime-correction.md](files-runtime-correction.md), [07-readme-original.png](../evidence/files-correction/07-readme-original.png), [08-readme-revisited-after-external-change.png](../evidence/files-correction/08-readme-revisited-after-external-change.png), [09-readme-reopened-after-close.png](../evidence/files-correction/09-readme-reopened-after-close.png).

#### FILES-02 — Keep edits through save errors and external changes

**Not exercised.** Priority P1; follow-up owner `files_runtime_correction`. Existing records: `panel-tools-001`, `panel-tools-002`.

- **Trigger:** Edit a file, inject a write refusal, close/reopen; separately change disk externally while the editor is open.
- **Visible result required:** Unsaved text survives; stale buffers cannot silently overwrite newer disk contents.
- **Failure/recovery required:** The user sees the affected file, retry/reconcile options and confirmed outcome.
- **Cross-surface checks:** Repeat with two threads on one checkout, autosave overlap and late write completion.
- **Next runtime check:** Requires a working real editor-input path or separately documented injection to establish preconditions; do not treat direct Editor.type_text as end-to-end proof.
- **Still unverified:** No dirty buffer was established through public controls; save, write refusal, dirty close and external-write conflict are untested. A clean preview staleness branch is recorded under FILES-01.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### FILES-03 — Discover files and keep the tree current

**Partial runtime failure.** Priority P1; follow-up owner `files_runtime_correction`. Existing records: `panel-tools-003`, `panel-tools-004`.

- **Trigger:** Open directory tree, then create/rename/delete files and change Git status externally; inspect a directory beyond its cap.
- **Visible result required:** Visible tree and status refresh; omissions and truncation are explicit.
- **Failure/recovery required:** Deleted targets explain their disappearance and provide refresh/retry.
- **Cross-surface checks:** Switch project/checkouts, filter the tree and reopen cached directories.
- **Next runtime check:** Exercise Git status separately and inspect boundary-sized trees; verify external additions/removals become visible without deleting working state.
- **Observed branches:** After external addition/removal, expanded tree keeps the deleted child and omits the new one; collapse/re-expand does not refresh it.
- **Still unverified:** Git status color updates, file discovery limits, deep trees, ignored paths and alternate refresh routes remain untested.
- **Correction observations:** `Files tree stale children`.
- **Executed evidence:** [files-runtime-correction.md](files-runtime-correction.md), [10-tree-expanded-before-mutation.png](../evidence/files-correction/10-tree-expanded-before-mutation.png), [11-tree-after-external-mutation.png](../evidence/files-correction/11-tree-after-external-mutation.png), [12-tree-after-collapse-expand.png](../evidence/files-correction/12-tree-after-collapse-expand.png).

#### FILES-04 — Keep all open files reachable and restore the working set

**Partial runtime failure.** Priority P1; follow-up owner `files_runtime_correction`. Existing records: `panel-tools-005`, `panel-tools-006`.

- **Trigger:** Open enough files to overflow tabs, select/close an offscreen tab, restart and return.
- **Visible result required:** Every open file remains reachable and the chosen file/working-set preferences restore.
- **Failure/recovery required:** Missing file on restore produces a scoped recovery state without losing other tabs.
- **Cross-surface checks:** Open wide tools, narrow the panel, hide explorer and switch threads.
- **Next runtime check:** Verify tab-strip access and close at ordinary/narrow widths; separately establish restored content with independent state/native evidence.
- **Observed branches:** Eighteen kept long-named inner tabs produce only four visible labels. files-tab-17 lies at logical x = 3250, beyond the 1440 px window; the strip offers no overflow navigation. The tree still activates paths, so content is not unreachable by every route. Switching outer surfaces produced a blank headless body, which is inconclusive for tab restoration.
- **Still unverified:** Native tab overflow navigation, hidden-tab close, resize, restart working-set persistence and selected-tab restore remain unverified. Blank body after outer switch may be a headless paint failure and is not promoted as a restoration defect.
- **Correction observations:** `Files inner strip overflow`.
- **Executed evidence:** [files-runtime-correction.md](files-runtime-correction.md), [26-kept-tabs-overflow.png](../evidence/files-correction/26-kept-tabs-overflow.png), [28-files-restored-after-switch.png](../evidence/files-correction/28-files-restored-after-switch.png), [29-files-after-switch-settle.png](../evidence/files-correction/29-files-after-switch-settle.png).

#### FILES-05 — Open the correct representation and source location

**Partial runtime failure.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `panel-tools-007`, `panel-tools-008`, `link-affordances-04`.

- **Trigger:** Open Markdown/prose/image files and source links with line suffixes from tree and transcript.
- **Visible result required:** The file opens in a useful view at its requested location, with an editing or preview route appropriate to the entry action.
- **Failure/recovery required:** Unsupported content has an explicit alternative and preserves path context.
- **Cross-surface checks:** Compare tree, transcript chip, PR Code and Changes entry points.
- **Next runtime check:** Click actual fixture targets and capture destination/path/line; right-click file-chip fallback already reproduced as wrong whole-message menu.
- **Executed evidence:** [29-file-chip-context-menu.png](../evidence/headless/29-file-chip-context-menu.png).

### Changes

#### DIFF-01 — Inspect all of a change without clipped or misleading presentation

**Positive runtime branch only.** Priority P1; follow-up owner `files_runtime_correction`. Existing records: `panel-tools-009`, `panel-tools-010`, `panel-tools-011`.

- **Trigger:** Open a file with >120 changed lines, long expressions and whitespace-only edits; change wrapping/whitespace controls.
- **Visible result required:** All intended hunks are reachable or truncation is explicit; long unwrapped lines scroll; whitespace toggle matches its label.
- **Failure/recovery required:** Large or unsupported diff explains limits without allowing a false complete-review impression.
- **Cross-surface checks:** Resize the panel, use keyboard navigation and mark reviewed only after inspecting content.
- **Next runtime check:** Use native scrolling to inspect the actual 120-row boundary and separate Review route, then wrapping/whitespace controls; do not promote source cap solely from the first hunk screenshot.
- **Observed branches:** Changes displays a full +160/−160 diffstat and initial hunk region for the controlled file. Source still caps panel hunk rows at 120.
- **Still unverified:** No working headless scroll input reached the omission line or final sentinel; runtime truncation reachability and full Review route remain unverified. Wrap/no-wrap, long lines, Hide whitespace and complete diff inspection remain untested.
- **Correction observations:** `Files long diff initial region only`.
- **Executed evidence:** [files-runtime-correction.md](files-runtime-correction.md), [14-changes-long.png](../evidence/files-correction/14-changes-long.png), [17-changes-pagedown.png](../evidence/files-correction/17-changes-pagedown.png).

#### DIFF-02 — Keep review state fresh and handle Git actions visibly

**Not exercised.** Priority P1; follow-up owner `files_runtime_correction`. Existing records: `panel-tools-012`, `panel-tools-014`.

- **Trigger:** Open Changes, mark reviewed, modify disk or comparison scope, then stage/unstage with success and failure fixtures.
- **Visible result required:** Diff contents and reviewed marks track the actual revision; stage/unstage visibly affects only the intended target.
- **Failure/recovery required:** Git lock/permission failure leaves a visible error and retry, with no false success.
- **Cross-surface checks:** Agent edits while another file is reviewed; switch thread on the same checkout.
- **Next runtime check:** Scratch Git repo and temporary index.lock; real rendered actions plus git status and frame capture.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Terminal

#### TERM-01 — Open, type, interrupt and close a working terminal

**Not exercised.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `panel-tools-015`, `input-focus-004`.

- **Trigger:** Open Terminal through each offered entry, type a harmless command, interrupt it, switch tabs, close and reopen; repeat with invalid shell.
- **Visible result required:** A usable terminal owns typing and shows output; closing it leaves the parent conversation intact.
- **Failure/recovery required:** Startup and process errors appear beside a recoverable control.
- **Cross-surface checks:** Keep unsent main draft and running agent unchanged; test Return/Tab/clipboard separately.
- **Next runtime check:** Use a disposable shell and recorded output sentinels; invalid shell tests actual failure view.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Browser

#### BROWSER-UI-01 — Navigate and edit the address predictably

**Not exercised.** Priority P1; follow-up owner `parent native inspection`. Existing records: `BROWSER-04`, `BROWSER-05`, `BROWSER-06`, `BROWSER-07`.

- **Trigger:** Open two pages on one host, enter localhost without scheme, begin editing a URL during redirect, then open a new tab and use Back/Reload.
- **Visible result required:** Tab identity is distinguishable; address editing is stable; toolbar actions target the visible page.
- **Failure/recovery required:** Invalid input and failed navigation have a clear error/retry preserving the typed destination.
- **Cross-surface checks:** Switch thread/tab, hide/show panel and change panel size during navigation.
- **Next runtime check:** Native CEF fixture with same-host titles, redirect delay and controlled navigation; no headless website claim.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### BROWSER-UI-02 — Use a normal website without host input stealing

**Not exercised.** Priority P1; follow-up owner `parent native inspection`. Existing records: `BROWSER-01`, `BROWSER-02`, `BROWSER-03`, `input-focus-001`, `input-focus-006`.

- **Trigger:** Use page forms, Tab/Enter/Escape, clipboard/IME, new-window links and JavaScript confirm/prompt.
- **Visible result required:** The website receives normal input and visible dialogs/new destinations; the main draft is never sent or altered.
- **Failure/recovery required:** Blocked unsupported behavior explains itself and offers a usable route out.
- **Cross-surface checks:** Keep sentinel draft, switch active tabs, right-click links/images/fields and return focus.
- **Next runtime check:** Actual CEF local fixture and native input are required; prior browser Observe success was not a completed interaction test.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### BROWSER-UI-03 — Keep browser chrome coherent across loading, empty and error states

**Partial runtime failure.** Priority P1; follow-up owner `parent native inspection + visual_quality_correction`. Existing records: `BROWSER-08`, `BROWSER-12`, `BROWSER-15`, `quality-correction-002`, `quality-correction-003`.

- **Trigger:** Open empty Browser, load a page slowly, navigate successfully, then fail navigation.
- **Visible result required:** Toolbar/address/page proportions remain stable and legible; status and recovery are attached to the relevant action.
- **Failure/recovery required:** Retry can be found and used without reading a large block of implementation-oriented text.
- **Cross-surface checks:** Compare shell tab row, nested page tabs, panel heading, browser toolbar and usable page viewport.
- **Next runtime check:** Capture native address hover/focus and controlled slow-load/error sequences at a common width; check page identity and add/close ownership. Preserve the existing loaded/loading/empty observations and their installed-version limits.
- **Observed branches:** Saved rendered chrome has a shared Browser tab, repeated blue Browser title inside an inset card, an inner page-tab row and then the toolbar. Parent independently observed the same stack with a loaded native example.com page. The empty address area has no prompt or clear enclosing location-control affordance in saved headless captures. Host-unavailable text appends edit/reload advice that cannot resolve lack of an embedded browser host. This is an error-presentation failure, not a native loading failure. Parent follow-up also observed native Loading with Annotate removed, Address shifted left and an added status row, followed by the empty-address state after page close. [Native evidence and limits](../evidence/native-correction.md).
- **Still unverified:** Native address hover/focus, controlled slow-load timing, invalid URL/network-error recovery and narrow-width legibility remain unverified. Native loaded/loading/empty corroboration is recorded in evidence/native-correction.md without a saved native image path. Loading geometry now has a bounded native observation. This does not upgrade schemeless-address, input ownership or error-recovery claims.
- **Correction observations:** `VQ-C01`, `VQ-C02`, `VQ-C03`, `quality-correction-002`, `quality-correction-003`.
- **Executed evidence:** [visual-quality-correction.md](visual-quality-correction.md), [13-browser-after-files.png](../evidence/panel-correction/13-browser-after-files.png), [15-browser-after-changes.png](../evidence/panel-correction/15-browser-after-changes.png).

#### BROWSER-UI-04 — Return to the same page after hiding and restarting

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `BROWSER-11`, `BROWSER-13`.

- **Trigger:** Navigate multiple pages, scroll and edit a harmless field, hide/reclaim the page, return, then restart.
- **Visible result required:** The visible recovery contract for history, scroll and page state is clear; persistent profile problems have a usable recovery route.
- **Failure/recovery required:** Expired auth/cache or unavailable target is distinguishable from an empty page.
- **Cross-surface checks:** Switch threads and leave Browser hidden while agents run.
- **Next runtime check:** Native CEF fixture plus controlled reclamation/restart; do not infer page restoration from URL metadata.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Links

#### LINK-01 — Open, copy and dismiss the target actually clicked

**Partial runtime failure.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `link-affordances-01`, `link-affordances-02`, `link-affordances-03`, `link-affordances-04`, `link-affordances-05`, `link-affordances-06`, `link-affordances-07`.

- **Trigger:** Activate ordinary URLs, issue references, file chips, fragments and PR badges through click, right-click, modifiers and keyboard.
- **Visible result required:** All routes preserve the resolved target and offer appropriate destinations; menus dismiss and restore focus.
- **Failure/recovery required:** Unsupported destination gives visible feedback instead of leaking internal schemes or doing nothing.
- **Cross-surface checks:** Compare transcript, Files, sidebar, PR details and browser chrome; preserve unsent text.
- **Next runtime check:** Web menu composition, file-chip fallback and Escape failed in bounded checks. Remaining destinations/copy values/modifiers require actual activation checks.
- **Executed evidence:** [28-web-menu-actual.png](../evidence/headless/28-web-menu-actual.png), [29-file-chip-context-menu.png](../evidence/headless/29-file-chip-context-menu.png), [30-web-menu-after-escape.png](../evidence/headless/30-web-menu-after-escape.png).

### Permissions

#### PERM-01 — Answer the right request with ordinary typing and truthful settings

**Runtime inconclusive.** Priority P1; follow-up owner `forms_runtime_correction`. Existing records: `PS-01`, `input-focus-007`, `coverage-quality-03`.

- **Trigger:** Receive a permission card or required form, type valid text beginning y/n/a, choose options, submit and continue the turn.
- **Visible result required:** Typing edits fields, explicit controls decide, and displayed sandbox/policy matches the running session.
- **Failure/recovery required:** Invalid/failed answers retain values, explain errors and permit correction.
- **Cross-surface checks:** Repeat async question during streaming and a child decision while main is focused.
- **Next runtime check:** Establish the nested Input painted bounds and actual hit through native input or an accurate public harness seam, then submit valid values for the exact request ID. Preserve the working validation/cancel/deny evidence.
- **Observed branches:** A valid required two-string schema arrived through the production Claude adapter; empty Submit displayed required validation. Public form selectors identify nonzero parent sections, not nested Input bounds. Click/type attempts entered the Main composer, but an actual nested Input hit was not established. Saved headless images cannot distinguish absent painting from clipping or incomplete repaint. Cancel sent matching action=cancel. A separate interaction_required approval displayed its command and waiting state; explicit Deny sent the matching denial and the fixture completed.
- **Still unverified:** Valid answer payload/association, actual Input visibility and hit geometry, field focus recovery, field-to-field Tab/Enter, asynchronous questions, child decisions, Allow and native UI remain unverified. This is inconclusive for a product form defect; missing labels/inputs and wrong-target typing must not be promoted without resolving the hit/paint ambiguity. input-focus-007 approval shortcuts and PS-01 Codex sandbox truthfulness were not tested.
- **Correction observations:** `PERM-01-question-required-form`, `PERM-01-approval`, `focus-and-composer-ownership`.
- **Executed evidence:** [forms-runtime-correction.md](forms-runtime-correction.md), [02-form.png](../evidence/forms-correction/02-form.png), [03-required-validation.png](../evidence/forms-correction/03-required-validation.png), [04-after-form-submit.png](../evidence/forms-correction/04-after-form-submit.png), [08-coordinate-click.png](../evidence/forms-correction/08-coordinate-click.png), [06-after-deny.png](../evidence/forms-correction/06-after-deny.png), [provider-frames.jsonl](../evidence/forms-correction/provider-frames.jsonl).

### Settings

#### SETTINGS-01 — Choose provider defaults that match the next session

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `PS-04`, `PS-05`, `PS-07`.

- **Trigger:** Set provider/model/permissions, start a fresh thread, inspect effective launch; simulate unavailable CLI/auth/session.
- **Visible result required:** Next session uses the displayed defaults and setup failures identify the actual recovery step.
- **Failure/recovery required:** Unavailable native session retains context and offers a same-thread recovery route where supported.
- **Cross-surface checks:** Compare existing session changes with new-thread defaults and restart.
- **Next runtime check:** Fake provider launch arguments and supported-model responses, then real settings UI; no real login or remote completion.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

#### SETTINGS-02 — Apply remote settings to the machine being controlled

**Not exercised.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `PS-02`, `PS-03`.

- **Trigger:** Connect isolated remote machine, change defaults and inspect provider update/version UI.
- **Visible result required:** Displayed machine identity, saved settings, CLI target and next session agree.
- **Failure/recovery required:** Connection/write failure stays scoped to that remote machine and preserves prior values.
- **Cross-surface checks:** Switch between local and remote while dialog open; restart remote session.
- **Next runtime check:** Requires isolated remote fixture; source-only risk today. No operator remote settings should be changed.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Persistence

#### RESTORE-01 — Resume a coherent workspace after restart

**Partial runtime failure.** Priority P1; follow-up owner `primary-agent-follow-up`. Existing records: `coverage-quality-05`, `panel-tools-006`, `subagents-background-09`, `quote-attachments-005`, `composer-02`.

- **Trigger:** Set window geometry, selected thread/tool/file/agent, quote-rich draft and reading position, then restart.
- **Visible result required:** The workspace resumes the chosen state or names recoverable exceptions; it does not silently land on Background or discard context.
- **Failure/recovery required:** Missing resource/path/provider affects only that resource with a clear recovery path.
- **Cross-surface checks:** Compare application restart, renderer-only restart, thread Park/reopen and panel close/reopen.
- **Next runtime check:** Build a combined owned workspace fixture and test normal Quit/restart plus interrupted restart; preserve individual context and targeted recovery.
- **Observed branches:** Panel width persists incorrectly through app restart; separately, a never-sent draft disappears across window-process restart while an existing-thread draft restores. Separately, one existing-thread whole-context quote plus ordinary commentary survives a full isolated server/window restart. This does not repair the never-sent pane failure.
- **Still unverified:** Normal Quit, coherent mixed-surface restoration, file working set, selected agents, attachments, model/checkout context and error recovery are not certified.
- **Executed evidence:** [panel-runtime-correction.md](panel-runtime-correction.md), [draft-lifecycle-runtime-correction.md](draft-lifecycle-runtime-correction.md), [03-unsent-draft-before.png](../evidence/draft-lifecycle-correction/03-unsent-draft-before.png), [06-new-draft-after-restart.png](../evidence/draft-lifecycle-correction/06-new-draft-after-restart.png), [attachment-runtime-correction.md](attachment-runtime-correction.md), [19-draft-before-restart.png](../evidence/attachment-correction/19-draft-before-restart.png), [20-after-server-restart.png](../evidence/attachment-correction/20-after-server-restart.png).

### Accessibility

#### ACCESS-01 — Operate existing controls and honor reduced motion

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `motion-006`, `motion-007`, `input-focus-003`.

- **Trigger:** Traverse visible controls by keyboard/assistive technology, then change system reduced-motion preference while app remains running.
- **Visible result required:** Controls expose meaningful names/roles/state and system motion preference updates as intended.
- **Failure/recovery required:** Disabled/unavailable actions are perceivable with focus recovery.
- **Cross-surface checks:** Menus, tab strips, citation actions, browser controls, PR segments and overlays.
- **Next runtime check:** Native platform check; static source plus headless default key dispatch do not establish assistive-technology or OS preference behavior.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

### Operational flows

#### OPS-01 — Start a requested provider through the documented CLI

**Partial runtime failure.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `coverage-quality-09`.

- **Trigger:** Use advertised multiword provider-argument syntax, then corrected repeated-flag syntax.
- **Visible result required:** Help and parser agree, the requested model receives its exact arguments, and the worker is discoverable/closable.
- **Failure/recovery required:** Rejected syntax fails clearly before starting a worker.
- **Cross-surface checks:** Check worker cleanup and parent thread identity.
- **Next runtime check:** Original --args multiword form failed; repeated --args= values succeeded. This is a narrow runtime failure, not a product-wide verification substitute.
- **Executed evidence:** [runtime-observations.md](../evidence/runtime-observations.md).

#### OPS-02 — Operate an existing schedule through its lifecycle

**Not exercised.** Priority P2; follow-up owner `primary-agent-follow-up`. Existing records: `coverage-quality-06`.

- **Trigger:** Create an available recurring schedule, inspect next-run time, edit, pause, resume and observe one deterministic execution.
- **Visible result required:** The chosen recurrence and next execution are visible; execution appears once on the correct target with outcome.
- **Failure/recovery required:** Invalid recurrence, missed run and provider failure are recoverable and explained.
- **Cross-surface checks:** Compare timezone, project target, restart and an already-running prior execution.
- **Next runtime check:** No runtime schedule check has run. One-time/sub-hour schedules are separate capability scope and do not determine whether existing recurrence works.
- **Executed evidence:** none recorded for this complete journey in this snapshot; source risks remain in the mapped records.

## Optional capabilities are a separate decision

The original inventory classified 28 records as capability candidates. Twenty-seven remain outside this quality verdict. Their absence does not reduce a completion score for existing behavior. `link-affordances-01` is the explicit exception: the user specifically requires an **Open in browser** choice when right-clicking a link, so that required result stays in LINK-01 even though the earlier inventory called it a candidate. Existing loading/empty/focus states still need review; mentioning a related candidate in a journey does not silently authorize every larger optional feature described in that record.

- `BROWSER-09` — Viewport size is inseparable from panel size.
- `BROWSER-10` — Find, DevTools, hard reload, and page appearance controls are absent.
- `BROWSER-12` — An empty browser provides neither useful guidance nor recent/local destinations.
- `BROWSER-14` — Browser capture is annotation-oriented and lacks direct recording/export affordances.
- `BROWSER-16` — No browser-chrome escape to the system browser.
- `composer-05` — The composer cannot choose queue versus steer per message.
- `composer-08` — Large pastes stay inline with no composer length guard.
- `composer-10` — The mention menu has no PR, thread or model-reference search.
- `input-focus-005` — No workspace-area focus cycle or per-surface focus restoration connects the composer and panel.
- `panel-tools-013` — Changes lacks full diff navigation and comparison controls present in T3.
- `panel-tools-016` — Terminal surfaces cannot split into simultaneous shells.
- `panel-tools-017` — Terminal output has no URL or file-path opening flow.
- `pr-navigation-03` — There is no standalone PR inbox or project-scoped PR browsing destination.
- `pr-operations-010` — No native comment, reply, review verdict or resolution workflow.
- `pr-operations-011` — PR metadata cannot be edited or managed in the native panel.
- `pr-operations-013` — Lifecycle actions lack inverse operations and branch maintenance.
- `projects-navigation-010` — Pinned rows cannot be reordered directly.
- `PS-06` — Provider choices do not include T3's additional drivers or named provider instances.
- `PS-08` — Provider defaults have no project override or fallback-model settings.
- `transcript-002` — Long transcript retention has no equivalent of earlier-turn paging.
- `transcript-007` — Markdown math is displayed as code instead of typeset formulas.
- `transcript-008` — Transcript Markdown has no inline audio or video playback.
- `composer-12` — There is no prompt stash independent of the active draft.
- `composer-14` — There is no integrated voice-input workflow.
- `composer-15` — A first prompt cannot fan out to multiple chosen models.
- `coverage-quality-07` — No first-class guest thread participation was found.
- `coverage-quality-08` — T3 device panels have no Ferrite panel counterpart.

The six original product-choice records also remain decisions rather than automatic defects. Where an existing choice affects safe operation (queue Stop, parking behavior, attachment meaning), the relevant journey checks that it is coherent, disclosed and recoverable; it does not assume T3's choice must be copied. The primary audit owns reconciling those choices with the user’s stated interaction requirements.

## Infrastructure evidence cannot substitute for the product

- `coverage-quality-01` — UI interaction tests do not gate ordinary PR CI.
- `coverage-quality-02` — Some UI tests bypass real keyboard and opener dispatch.
- `coverage-quality-04` — Default UI checks do not establish native Chromium behavior.

CI presence, helper tests, and a successful build are supporting evidence. They do not certify production event dispatch, the native browser bridge, correct target identity, usable visual hierarchy, or motion. This matrix worker made documentation changes only and ran no application tests or builds. Parent reconciliation adopted four new records and scoped runtime evidence into the consolidated tally. It validated case IDs and evidence paths against the saved audit.

The audit can close an individual case only after its visible outcome, failure/recovery and cross-surface checks have evidence, with any remaining environment-specific limits stated. Until then the primary audit retains the work. Neither a large finding count nor a list of implemented components supports “the app has been fully investigated” or “the feature works.”
