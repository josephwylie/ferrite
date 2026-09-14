# Ferrite full UX audit — 14 September 2026

Ferrite has a coherent desktop cockpit and substantial interaction infrastructure, but users cannot consistently trust what typing, parking, project navigation, or reviewing a change will do. The largest problems are operational: actions can target the wrong surface or repository, instructions can disappear, and review information can be misleading or inaccessible.

**42 distinct findings: 23 P1 major, 19 P2 minor.** No P0 claim and no P3 polish backlog. The priority order below favors unintended actions, lost work, and routine multi-agent supervision over appearance preferences. A narrow required form can be blocked even though the whole application is not unusable; P1 includes those workflow failures.

**Audit branch:** `audit/full-ux-2026-09-14` · **Source baseline:** `fbc03f3` · **Product changes:** none. Only audit documentation and evidence are added on this branch. The original `main` checkout, its uncommitted README/ADR edits, and the existing prompt-queue worktree were left intact. There was no merge, deployment, provider switch, live prompt submission, or real approval test.

## What should be addressed first

1. **Restore intentional control.** Enter in Settings submits a hidden draft; Cmd+W parks the hidden Thread. Starting “yes, but first…” in an invited approval reply authorizes on the first letter. Normal MCP form input and dense-grid approvals lose keyboard focus. See UX-01–04 and UX-10.
2. **Make workspace and lifecycle actions match their labels.** The Group’s advertised Park shortcut removes membership and keeps the Session running. A new Thread can switch a shared checkout underneath other agents. Browsing Projects can retarget an already written, hidden draft. See UX-05–07 and UX-18.
3. **Preserve instructions.** Parking loses the unsent prompt and attachments. Certain refused sends clear the prompt before any recoverable history is written. Windows select-all and multiline editing also need correction. See UX-08–09, UX-17, UX-24.
4. **Make review faithful.** Only the latest 200 blocks are reachable despite more retained history. Diff display/copy removes source indentation; Codex multi-file hunks lack filenames. See UX-11–13.
5. **Make supervision and choices dependable.** Handover consequences are missing from the model list, discovery can change the meaning of an open picker row, crashes bypass the bell, and overview status misses child work/failure. See UX-14–15 and UX-21–22.

These are examples of work users can encounter in current sessions. They are not a speculative catalogue of long labels or extreme viewport sizes.

## Method and boundaries

Exactly five subagents independently examined: (1) onboarding/navigation/workspaces/lifecycle, (2) Composer and provider controls, (3) transcript and activity reading, (4) Decisions/status/recovery, and (5) Settings/desktop behavior. The lead reviewed their evidence, checked major call chains, removed duplicates, inspected the running app, and requested bounded reproduction probes. Three duplicated findings were merged: Windows shortcuts, startup model defaults, and the Thread/subagent overview mismatch.

The source audit covers committed `fbc03f3`; pre-existing uncommitted documentation changes and the separate feature worktree were not silently included. Native observations came from the installed **0.3.0 development build** on macOS. Its exact compiled Git SHA is not exposed in About, so those observations corroborate source behavior rather than establish binary/source identity.

Evidence classes:

- **R — reproduced in the actual renderer/GPUI harness.** Existing fake Sessions, production keybindings/pump, and temporary test code in a disposable source copy; no live provider calls. Seven GPUI scenarios and one direct diff-renderer probe reproduced eight failures spanning seven consolidated findings.
- **V — observed in the running native app.** Main cockpit/model menu/Settings inspected, including native reproduction of blank search results and missing option-name matches. Search was cleared and Settings closed. No preference was changed.
- **E — isolated execution.** Actual Composer line/mention modules, generated Windows binding table, and a disposable Git repository. This verifies logic/commands, not a Windows GUI or real agent outcome.
- **S — source/contract verified.** Specific UI-to-state-to-provider/render paths, sometimes corroborated by existing tests. Timing-dependent picker behavior, native close/reopen, background-window acknowledgement, and actual provider refusals remain untested at the native/provider boundary.

Eight temporary probes assert the desired behavior and all fail against the audited source. That is successful defect reproduction, **not** a claim that eight existing repository tests failed. The existing full suite was not rerun: this is an audit with no product patch. The [full log](evidence/headless-probes.log), [reproduction instructions](evidence/reproduction.md), and [unapplied probe patch](evidence/headless-probes.patch) are preserved. The patch is evidence only; it has not been applied to this branch.

This is a native GPUI macOS/Windows desktop application. A web DOM/CSS detector, phone breakpoints, Android navigation rules, and touch-size checklists would not establish useful evidence here, so they were not used. The known v1 screen-reader limitation is recorded under constraints rather than repeated as new defects. No live provider authentication, network outage, permission change, macOS window close, Windows executable, sustained performance benchmark, or full every-state screenshot sweep was performed. Source verification covers these paths where listed; the audit is not exhaustive runtime certification.

## Priority index

Each linked entry includes the scenario, current behavior, practical impact, precise source lines, recommended correction, and verification limits.

| ID | Priority | Current user-visible problem | Ordinary trigger | Evidence |
|---|---|---|---|---|
| [UX-01](findings.md#ux-01) | P1 | Settings sends or parks the hidden Thread | Open Settings, then press Enter or Cmd/Ctrl+W. | R |
| [UX-02](findings.md#ux-02) | P1 | Typing a sentence can authorize a tool on its first letter | Start “yes, but first…” in the invited Decision reply field. | S |
| [UX-03](findings.md#ux-03) | P1 | Required MCP form input goes into the Composer | Click the required Count field, type 2, and submit. | R |
| [UX-04](findings.md#ux-04) | P1 | An approval breaks keyboard control in a normal four-pane grid | At 860×500, receive approval; y and Cmd+F stop working. | R |
| [UX-05](findings.md#ux-05) | P1 | The advertised Park shortcut leaves the agent running | Press Cmd+W on a Group member; membership is removed instead. | S |
| [UX-06](findings.md#ux-06) | P1 | A new Thread can switch the checkout under live Threads | Choose main or New branch in a shared checkout and send. | E/S |
| [UX-07](findings.md#ux-07) | P1 | Project navigation silently retargets a populated draft | Filter to another Project while a prepared draft is hidden. | S |
| [UX-08](findings.md#ux-08) | P1 | Parking loses the unsent prompt and attachments | Compose a follow-up, park in Solo, then reopen. | S |
| [UX-09](findings.md#ux-09) | P1 | Some refused sends erase the prompt | Submit after the project root disappears or replacement startup fails. | S |
| [UX-10](findings.md#ux-10) | P1 | A custom single-choice answer sends the old radio choice too | Pick an option, then change your mind with a custom answer. | R |
| [UX-11](findings.md#ux-11) | P1 | Earlier transcript history becomes unreachable after 200 blocks | Scroll to initial instructions in an ordinary extended Thread. | R |
| [UX-12](findings.md#ux-12) | P1 | Diff display and copying strip meaningful indentation | Review a Python, YAML, or Makefile hunk. | R |
| [UX-13](findings.md#ux-13) | P1 | Codex multi-file hunks omit filenames | Review an expanded fileChange containing two files. | S |
| [UX-14](findings.md#ux-14) | P1 | Provider handover is hidden inside an ordinary model choice | Select a model from the other provider after several turns. | V/S |
| [UX-15](findings.md#ux-15) | P1 | An open model picker can execute a different row after discovery | Leave the picker open while the model catalog changes. | S |
| [UX-16](findings.md#ux-16) | P1 | Selecting a file mention with spaces resolves the wrong path | Choose docs/Design Notes.md from @ completion. | E |
| [UX-17](findings.md#ux-17) | P1 | Windows Ctrl+A and Ctrl+W perform the wrong edits | Select-all prepends text; the documented close chord deletes a word. | E/S |
| [UX-18](findings.md#ux-18) | P1 | New Thread in this Group creates an ungrouped Thread | Use the Group header context menu and send. | S |
| [UX-19](findings.md#ux-19) | P1 | New-Thread defaults depend on which creation action you use | Set a default provider/model, then use Cmd+T or restart with all parked. | S |
| [UX-20](findings.md#ux-20) | P1 | Changing default model retains unsupported effort invisibly | Select a high effort, then a model with a shorter effort ladder. | S |
| [UX-21](findings.md#ux-21) | P1 | Background process crashes bypass the bell and toast | A working provider exits unexpectedly while another Thread is focused. | R |
| [UX-22](findings.md#ux-22) | P1 | Thread overview hides subagent failures and remaining work | Main ends while a child works/fails, especially behind +N overflow. | S |
| [UX-23](findings.md#ux-23) | P1 | macOS red-close has no window reopen path | Close the sole window, then activate Ferrite from the Dock. | S |
| [UX-24](findings.md#ux-24) | P2 | Multiline line-boundary editing acts on the whole prompt | Cmd+Backspace on the second line deletes the first instruction too. | E |
| [UX-25](findings.md#ux-25) | P2 | Interrupt can immediately continue queued work | Press esc interrupt with a follow-up already queued. | S |
| [UX-26](findings.md#ux-26) | P2 | Workspace choices invent main and omit real alternatives | Open a master-only repository and choose the offered main branch. | E/S |
| [UX-27](findings.md#ux-27) | P2 | New Thread can use a different Project from the visible Thread | Under All Projects, open B and press Cmd+T after working in A. | S |
| [UX-28](findings.md#ux-28) | P2 | Closing Project editor saves some edits and discards others | Change name and directories, then dismiss without Done. | S |
| [UX-29](findings.md#ux-29) | P2 | Project-directory edits leave active Sessions on the old scope | Add/remove an additional directory and continue the same Thread. | S |
| [UX-30](findings.md#ux-30) | P2 | There is no transcript Find; Cmd/Ctrl+F changes layout | Try to retrieve an earlier filename, instruction, or test failure. | S |
| [UX-31](findings.md#ux-31) | P2 | Expanding a tool at the live tail moves its header out of view | Open 20–30 lines of output in a compact Pane. | S |
| [UX-32](findings.md#ux-32) | P2 | The structured plan cannot be opened for inspection | Try to inspect upcoming steps behind a 2/4 progress meter. | S |
| [UX-33](findings.md#ux-33) | P2 | Source links discard their line destination | Open src/lib.rs:240 from an agent response. | S |
| [UX-34](findings.md#ux-34) | P2 | The next prompt removes the prior consolidated Turn changes | After implementation, ask to run tests, then review that turn. | S |
| [UX-35](findings.md#ux-35) | P2 | Subagent prompt Resend is a visible no-op | Click Resend on a child transcript prompt. | S |
| [UX-36](findings.md#ux-36) | P2 | Selecting a child replaces the owning Thread title | Read children in several Panes from the same repository. | S |
| [UX-37](findings.md#ux-37) | P2 | Settings search can show a blank panel for a valid result | Open About, search sandbox; only the category remains visible. | V/S |
| [UX-38](findings.md#ux-38) | P2 | Settings search cannot find visible option names | Search full access immediately after seeing that option. | V/S |
| [UX-39](findings.md#ux-39) | P2 | Codex model has no route back to CLI default | Select an explicit model, then try to return to the described default. | V/S |
| [UX-40](findings.md#ux-40) | P2 | Window size and placement reset at launch | Arrange Ferrite on a monitor, quit, and relaunch. | S |
| [UX-41](findings.md#ux-41) | P2 | Events are marked seen while Ferrite is behind another app | Leave the selected Thread working and switch to an editor. | S |
| [UX-42](findings.md#ux-42) | P2 | Settings save failures are hidden behind the modal | A save fails while Settings is open or the sidebar is collapsed. | S |

## Coverage by user journey

| Journey | What was examined | Main findings |
|---|---|---|
| First run / create a Thread | Project creation, no agent before first send, CLI defaults, draft reuse, model/effort initialization | UX-18–20, UX-26–27 |
| Choose where work happens | Project filters, shared checkout, branch/worktree choices, additional directories, save/cancel | UX-06–07, UX-26–29 |
| Organize many agents | Solo/Group creation, context menus, park/reopen/delete, Group membership/layout, focused Project | UX-05, UX-18, UX-22, UX-27 |
| Write and steer | Editing, prompt history, slash/@ completion, files/images, draft lifetime, send rejection, queued work, interrupt | UX-08–09, UX-16–17, UX-24–25 |
| Choose provider/model | Handover, discovery, model/default effort, disabled/busy transitions | UX-14–15, UX-19–20, UX-39 |
| Answer Decisions | Main/child routing, quick keys, approval/form/question focus, single/multi-select, generation/stale handling | UX-01–04, UX-10 |
| Read and review work | History horizon, follow/scroll anchoring, Markdown, code/diff fidelity, selection/copy, source links, plan, turn review | UX-11–13, UX-30–34 |
| Supervise child agents | Visible/overflow activity, failure state, Main identity, historical requests, retry, child prompt affordances | UX-22, UX-35–36 |
| Return to completed/failed work | Bell/toast, unexpected exits, deferred completion, app inactive/read acknowledgement, resume | UX-21–22, UX-41 |
| Configure the app | Settings modal, categories/search, defaults/effort, persistence failures, platform keyboard/window handling | UX-01, UX-17, UX-19–20, UX-23, UX-37–42 |

## Systemic causes

**Displayed intent and dispatched action have diverged.** A Group menu says “in this Group” but creates a loose draft; “Park” advertises a shortcut that leaves the Group; Settings looks modal but dispatches cockpit actions. These require agreement between the UI’s action contract and the underlying state transition.

**Focus ownership is inferred too broadly.** The renderer tries to keep a Composer focused even when L2 has removed it or an MCP input needs focus. The correct owner must follow the actual mounted interaction surface, including modal boundaries and all Decision kinds.

**View lifetime is being used as data lifetime.** Unsent prompts disappear with PaneView; the 200-block projection becomes the user’s whole accessible history; consolidated diffs disappear on the next prompt. Ephemeral presentation state should not determine the survival of the operator’s work or review evidence.

**Provider abstractions discard meaning at the UI boundary.** Native model menus lose section descriptions, Codex diff paths are available but not drawn, settings model/effort combinations become inconsistent, and discovered choices are dispatched by an unstable position. The data exists in several of these cases; the presentation contract does not preserve it.

**Main state is mistaken for whole-Thread state.** The bell correctly waits for background children, while navigation and compact panes can already look done. A cockpit overview needs Thread-level progress and failure semantics alongside Subject-local details.

## What is working

- First-run Project creation is explicit. Drafts do not start a provider until first send; failed initial workspace/startup setup retains the draft and exposes its error.
- The transcript has retained native entities, variable-height virtualization, stable row identities, offscreen selection, width anchoring, and wheel-driven live-follow detachment. These provide a sound base for fixing the separate history horizon.
- Provider handover preserves transcript and carries a digest; failed replacements do not falsely commit the replacement provider. Native queue receipts/cancellation handle important races, and ambiguous partial writes are not blindly resent.
- Decision handles are scoped to a Session generation; stale/historical requests are not answerable. Main/child histories, disclosure and selection are kept separate. Retry and Return to Main are explicit for unavailable child history.
- Completion accounting already defers for background descendants and excludes operator interrupts, continued queued work, and replayed history. This is worth keeping while fixing crash and unread presentation.
- Attachments use a robust serialization path for drag/drop and clipboard files, preserve selection/history state, and provide missing-file/preview behavior. The unquoted @-completion path is the specific inconsistency.
- Group layouts persist; Group revival rolls back partial failure. Bulk deletion is bounded by visible filtered parked rows, and dirty worktrees are not forcibly removed.
- The incumbent dark visual system is coherent and token-based. Static main text contrast is strong: TEXT/PANE 13.33:1, TEXT_2/PANE 7.54:1, TEXT_MUTED/MENU 4.92:1. This audit found no reason to propose a visual redesign.

## Technical quality assessment

The Impeccable technical checklist is adapted to desktop GPUI. Scores are qualitative source/limited-runtime judgments, not performance measurements or accessibility certification. The practical issue index above should drive decisions.

| Dimension | Score / 4 | Evidence and limit |
|---|---:|---|
| Accessibility and keyboard access | 0 | No full screen-reader support is an acknowledged v1 constraint. Independently actionable keyboard/form failures are reproduced. |
| Performance structure | 3 | Retained/virtualized rendering and background startup/discovery; no new measured latency defect. No sustained benchmark was run. |
| Appearance / theming | 3 | Coherent tokens and adequate principal text contrast; dark-only/fixed type remain current product constraints. |
| Desktop action conventions | 1 | Modal action leakage, Windows key collisions, and missing macOS reopen path. Windows/native-close evidence is source-level. |
| Adaptivity to cockpit density | 2 | Semantic zoom and resizable Groups exist; ordinary L2 Decisions break focus and compact overviews miss child work. |
| **Total** | **9/20** | **Poor under this technical rubric; significant task/control work remains.** The known screen-reader limitation contributes to the score but is not counted as a new audit finding. |

## Deliberate tradeoffs and exclusions

- **Unread state while inactive (UX-41)** and **interrupt continuing queued work (UX-25)** match deliberate current policies. They are identified as practical UX tradeoffs to revisit, not accidental implementation regressions. No demand for OS notifications is inferred.
- **Cmd/Ctrl+F fullscreen (UX-30)** is documented. The unmet need is retrieving content in a long Thread; the shortcut conflict compounds that gap.
- No speculative overflow/truncation findings, generic animation complaints, invented frame-rate regression, forced mobile redesign, or unsupported claims that real provider data was lost are included.
- No extra issue was counted for Composer wheel scrolling without native reproduction. Windows Ctrl+Shift+Left/Right was excluded after checking binding order. Startup Solo and no child-completion Notice are documented choices, not bugs.
- The audit does not reopen the settled v1 screen-reader decision. Larger text/appearance preferences are limitations to acknowledge, not automatically release blockers in this scope.

## Suggested follow-up order (no fixes performed)

1. **P1 — `$impeccable harden`:** intentional input/approval boundaries, Settings isolation, required form focus, draft recovery, stable choice identities.
2. **P1 — `$impeccable clarify` / `$impeccable shape`:** align Park/Leave/New Thread actions, workspace retargeting and shared-checkout consequences, provider handover, and defaults.
3. **P1 — `$impeccable harden` / `$impeccable adapt`:** restore accessible history, faithful diff paths/whitespace, L2 keyboard ownership, aggregate Thread attention.
4. **P2 — `$impeccable clarify` / `$impeccable harden`:** Settings search/default reset, plan/find/source navigation, turn review persistence, Project editor save/apply semantics.
5. **Final — `$impeccable polish`:** finish visual/focus feedback only after the behavior contracts are correct. Re-run the relevant audit reproductions after fixes.

These work packages can be addressed individually or together. This branch contains the audit only; none of the recommendations have been implemented.
