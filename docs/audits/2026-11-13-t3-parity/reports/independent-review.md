# Independent evidence and decision-trail review

Parent chronology note: this review is retained as delivered. Its zero-runtime statement and missing-partition observations describe the earlier review snapshot. Later source supplements, adjudications, pinned-source build, and real GPUI checks are recorded in the [final report](../README.md) and [runtime report](runtime-interactions.md). The reviewer did not perform or re-review those later runtime checks.

Reviewer: worker thread 1001 (Claude). Scope: read-only source inspection of both pinned trees plus the audit's own files. I did not run the app, run tests, or spawn workers.

- Ferrite: `62947f91d5cd6ee494aaae9ed09d794a853e5bab` (HEAD confirmed; only the audit directory is untracked).
- T3 Code: `/tmp/ferrite-t3-audit-source-20261113` at `74573fc29b901bafbf8f1dbb2eb29606da1fa448` (confirmed, clean).
- Ferrite paths below are repository-relative. T3 paths are relative to the T3 checkout at that commit.

**Nothing in this audit is runtime-reproduced.** Every verdict below is a source verdict. `evidence/runtime-observations.md:10` records that the installed 0.4.0 binary's source commit is not established, so even a correct source finding may not describe what the user is running.

## Verdicts on the seven claims

| # | Claim | Verdict | Qualification |
|---|---|---|---|
| 1 | PR chips open Browser, invisibly on nonfocused threads | VERIFIED (source) | Panel-hidden case not checked |
| 2 | Agent drill drops history, decisions and events; reopen gives a blank body | VERIFIED (source) | History loss needs an unretained child; P1 looks high |
| 3 | Host actions pre-empt browser and empty-panel raw key handlers | VERIFIED (source) | Worse than stated for Enter and Escape |
| 4 | Failed save can discard the buffer; stale save can overwrite external changes | VERIFIED (source) | Both rated P0; neither fits P0. Stale overwrite is not a T3 parity gap |
| 5 | Idle send refusal loses already-cleared text | VERIFIED (source) | Narrow preconditions |
| 6 | Sidebar and panel widths snap by explicit policy | VERIFIED (source) | Documented intent, not a defect |
| 7 | CI excludes cockpit and pane tests | VERIFIED for ordinary PR CI only | A path-triggered workflow runs a small subset |

### 1. PR chips open Browser, invisibly on nonfocused threads: VERIFIED

- `crates/ferrite-cockpit/src/cockpit.rs:12806-12819`: a plain click on the chip calls `open_pr(thread)`; cmd-click opens the system browser.
- `cockpit.rs:1826-1831`: `open_pr` issues a browser `Open` for that thread, and returns silently when the PR has no URL.
- `cockpit/browser.rs:218-229` and `637-647`: the reveal is queued for the clicked thread and acts on that thread's pane. With no pane it stays queued.
- `cockpit/panel.rs:204-211`: the visible panel belongs to the focused pane only. Nothing in the path focuses or activates the clicked thread.
- T3 activates the thread first: `apps/web/src/components/sidebar/SidebarThreadRow.tsx:962-969`, `apps/web/src/lib/openPullRequestLink.ts:74-137`.

Not checked: whether a hidden panel is opened by the reveal. `pr-navigation-01`'s acceptance check lists that case, which is the right place for it.

### 2. Agent drill: VERIFIED, with a condition on the history part

- No history load: `cockpit/agent_pane.rs:80-111` never calls `ensure_subject_history` or `set_visible_subject`. The inline path does (`cockpit/subagents.rs:397-400`, `433-436`, `1055-1071`).
- Events dropped: `agent_pane.rs:127-143` handles only `ToggleDisclosure`. The other variants in `crates/ferrite-pane/src/transcript.rs:273-311` (open reader, open agent, open background, open thread, open changes, send) go nowhere.
- Decisions dropped: `cockpit/side_chat.rs:477-482`, `527`, `564`. The compact transcript is built without the Decision strip, subagents or reply.
- Blank on reopen: `cockpit/palette.rs:1294-1298` only calls `reopen_closed`; `cockpit/surfaces.rs:48-53` lists Agent as reopenable; `cockpit/panel.rs:313-319` releases the drill on close; `agent_pane.rs:203-207` renders an empty `div()` for a missing drill.

Condition: history is only missing when the child is not retained. `crates/ferrite-core/src/engine/history.rs:112-137` returns early for a retained child, and the default is `max_children: 128` (`crates/ferrite-core/src/activity.rs:330-338`). The audit does not establish how a user reaches an unretained child in ordinary use (restart is the obvious candidate and is unproven).

### 3. Host actions pre-empt raw key handlers: VERIFIED

- Dispatch order: `vendor/gpui-pre/src/window.rs:6152-6168` dispatches bound actions first and returns if propagation stopped; key listeners run after. `window.rs:6549`: actions stop propagation in the bubble phase by default.
- Bindings: `crates/ferrite-ui/src/keymap.rs:185` (enter to `cockpit::Submit`), `192` (escape to `Interrupt`), `309-310` (tab, shift-tab), all in context `Ferrite` (`keymap.rs:52`), which is the root.
- Empty panel: raw handler at `cockpit/panel/strip.rs:687-691`, `716-743`. `submit` (`cockpit.rs:5486-5606`) neither propagates nor checks panel focus, so Enter can send or queue the main draft (`5552-5597`).
- Browser: raw handler at `crates/ferrite-pane/src/visual/element.rs:270-278`. It sits inside the preview container, which carries the `Reader` context (`cockpit/panel/preview.rs:498-500`). `keymap.rs:537` binds escape to `reader::Close` there, and the handler at `preview.rs:515-519` closes the preview. By source, Escape in a page closes the browser surface and Enter submits the conversation draft.
- The only test of the empty-panel keys calls the handler directly and bypasses dispatch: `cockpit/tests/tools.rs:202-217`.

This is the claim the failed embedded-browser fixture was meant to settle. It remains the highest-value runtime check in the audit.

### 4. Editor save: VERIFIED, priorities wrong

- Error swallowed: `cockpit/tools/editor.rs:136-141` clears `dirty` only on success and reports nothing on failure.
- Close drops the buffer: `cockpit/tools/files.rs:256-263` removes the tab, then saves.
- Stale overwrite: the file is read once at open (`files.rs:207`); autosave fires 800 ms after an edit (`editor.rs:41`, `219-227`). No mtime check, watcher or reload exists under `cockpit/tools/`.
- T3 keeps unconfirmed contents and retries on failure (`apps/web/src/components/files/fileSaveSessions.ts`, `fileSaveCoordinator.ts`). I found **no** conflict, mtime or version detection in either file.

### 5. Idle send refusal loses text: VERIFIED

- `cockpit.rs:5553` takes the draft; `5591-5593` restores it only on the queue path; `5595` calls a `send` that returns unit (`crates/ferrite-core/src/cockpit.rs:1747-1760`).
- Refusals after the take: `crates/ferrite-core/src/engine.rs:2547-2549` (bootstrap in flight, silent), `2557-2559`, `2564-2567`, `2568-2570`. A delivery failure does keep the text (`2576-2577`).
- `crates/ferrite-pane/src/composer.rs:517-528`: the take also clears files, replies and chips.

All triggers are narrow timing windows. `composer-01` at P1 is defensible on the data-loss reading, generous on frequency.

### 6. Widths snap by policy: VERIFIED as intent

- `crates/ferrite-ui/src/theme.rs:3531-3533` states that the sidebar and panel land on their frame; `crates/ferrite-ui/src/motion.rs:33`, `35`, `70`.
- T3 animation is opt-in and off by default: `packages/contracts/src/settings.ts:164` (`DEFAULT_PANEL_ANIMATION_DURATION_MS = 0`), gated classes in `apps/web/src/components/ui/sidebar.tsx:388`, `400`.

BRIEF.md asks for both facts to be recorded, since the user wants motion. Category `motion` at P2 is acceptable; calling it a defect or parity gap would not be.

### 7. CI excludes cockpit and pane tests: VERIFIED for ordinary PR CI

- `.github/workflows/ci.yml:87-93`, `124-133`: only `ferrite-core`, conditionally `ferrite-reviewr`, and a build.
- `.github/workflows/browser-window.yml:219-237` runs a narrow cockpit and pane subset, triggered only by browser script paths or manual dispatch (`14-30`).

`coverage-quality-01` already says "ordinary PR CI". Any summary that drops the qualifier is an overclaim.

## Attention flags

1. **Two P0s do not meet the P0 definition.** BRIEF.md reserves P0 for data loss or a broadly unusable critical workflow. `panel-tools-001` needs a write failure at close, which is rare. `panel-tools-002` needs an external change followed by a keystroke in a stale buffer: plausible when an agent edits an open file, but conditional and unreproduced. Recommend P1 for `-002` and P2 for `-001` unless runtime evidence shows otherwise.
2. **`panel-tools-002` is not a T3 gap.** T3 shows no stale-write protection either. Keep it as a Ferrite defect and remove any parity framing.
3. **`subagents-background-01` P1 depends on an unproven trigger.** State the eviction or restart precondition in the finding, or lower it.
4. **Claim 3 is understated.** `input-focus-001` and `-002` should say that Enter can send the user's draft and, for the browser, that Escape closes the surface. These two share one root cause and one fix; count them once in the tally.
5. **Duplicates to merge before tallying.**
   - `pr-navigation-01`, `-02`, `-05`, `-06`: one root (`open_pr` routes to Browser for the clicked thread).
   - `motion-001`, `motion-002`, `panel-tools-018`: the snap policy.
   - `motion-010` and `BROWSER-08`.
   - `subagents-background-01`, `-03`, `-09` and `panel-tools-006`: restore and reopen of released surfaces.
   - `coverage-quality-02` and the test bypass in claim 3.
6. **Possible missed sibling of the blank reopen.** SideChat and File surfaces are also reopenable while `panel.rs:316-319` releases their resources on close. I did not trace them. Worth one check.
7. **T3 features treated as requirements.** Several P2 `missing-capability` items (`composer-12`, `-14`, `-15`, `transcript-007`, `-008`, `PS-06`, `BROWSER-13`, `-14`, `panel-tools-016`) show that T3 has a feature, not that Ferrite needs it. I validated their T3 citations mechanically only (path and line range exist), not semantically. Mark them as candidates for the user to choose from.
8. **Citation hygiene.** Of 857 citations in the twelve slice JSONs, all paths and line ranges resolve in the pinned trees. Exceptions to the brief:
   - `coverage-quality-07` has a prose sentence in `ferrite_evidence` instead of a path.
   - Six slices (browser, composer, panel-tools, pr-operations, projects-navigation, transcript) cite T3 as bare paths, not commit-pinned links.
   - ID formats vary (`BROWSER-01`, `PS-01`, two-digit and three-digit suffixes).
9. **Partition 13 has no report.** BRIEF.md now lists `link-affordances`; no `reports/link-affordances.*` exists. The completion predicate is unmet until it returns evidence or an explicit gap.

## Decision trail

**The complete parent transcript is not available to me.** I reviewed `decisions.tsv` only, so I cannot say whether the parent's unrecorded reasoning was sound.

- The log has three rows for an audit with thirteen partitions, one reviewer and at least three tool failures.
- Row 1 says "12 source investigations"; row 3 adds a thirteenth. Row 1 was not updated and both remain `open`.
- Row 2 cites `evidence/browser-fixture.html` as evidence for failed observation. The evidence is `evidence/runtime-observations.md`; the fixture proves nothing about the failure.
- Row 3 rests on a user report (Copy-only link menus) that appears nowhere in `evidence/`. Record the report verbatim or mark it as unverified user testimony.
- Missing rows: worker launches and slice arrivals; the parent's own checks of high-impact claims; deduplication; priority changes; the `--args` parse failure and its workaround; creation and closure of thread 1001; the choice not to establish the installed binary's commit.
- No row records a contradiction or its resolution, which the completion predicate requires.

## What should change in the audit

1. Head the final artifact with the evidence limit: zero runtime-reproduced findings, installed build commit unknown.
2. Establish the installed binary's commit, or build the pinned commit, before attributing any user-reported symptom to a source finding.
3. Run three runtime checks first, in this order: browser and empty-panel Enter with a sentinel draft (claim 3); PR chip click on a nonfocused thread (claim 1); reopen of a closed Agent drill (claim 2).
4. Re-rate `panel-tools-001`, `panel-tools-002` and `subagents-background-01` as above.
5. Merge the duplicate groups and publish the tally after merging.
6. Label T3-only capabilities as options, separate from Ferrite defects.
7. Normalise T3 citations to commit-pinned links and fix `coverage-quality-07`.
8. Close or explicitly gap partition 13.
9. Backfill `decisions.tsv` with the missing rows and correct row 2's evidence path.
