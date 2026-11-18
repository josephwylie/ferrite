# Run T3 native journeys against their source implementation

## Overview

The goal is to identify where Ferrite falls short of a complete working experience. Use native T3 journeys to establish the comparison. A visible control does not pass a case until its target, content, and resulting state agree with the implementation.

This checklist contains 100 source-derived journeys at T3 commit `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. The author read source and local Ferrite Git metadata. The author did not launch T3, control its UI, read credentials, run a model, run tests, or build software for this checklist. Every journey starts as **not-run**. The parent owns live evidence and the authorized Sol and Sonnet subscription runs.

Throughput checkpoint. n/a, read-only investigation.

## Key concepts

A case has a visible entry, a controlled fixture, steps, observable assertions, cleanup, and source bindings. Each binding names a symbol and the state or branch that matters. Record the installed T3 version before comparing it with this pinned source. A version mismatch is an uncertainty, not a demonstrated defect.

Use these result states.

| State | Meaning |
| --- | --- |
| not-run | No native evidence exists yet. |
| pass | The actual target, content, and state satisfy the assertion. |
| fail | The native result contradicts the assertion under a valid fixture. |
| fixture-blocked | The required content, capability, permission, or environment is unavailable. |
| version-mismatch | Installed behavior cannot yet be correlated with this pinned implementation. |
| inspect-only-complete | The menu or draft was inspected and canceled. The write path remains untested. |

A screenshot proves the visible frame. It does not prove motion, live output, a stopped process, refreshed data, or a persisted save. Use a frame sequence for motion. Use successive output observations for streaming. Read the saved scratch file for persistence. Confirm process or run completion for cleanup.

## How the run works

### Prepare owned fixtures

1. Create two audit-named scratch threads in an independent disposable local Git repository. Use unique text markers per thread and file.
2. For real read-only PR data, use an independent disposable local clone or copy of Ferrite with its GitHub origin. Keep active Ferrite checkout untouched. The local merge history identifies PR 189 and PR 187 without any host write.
3. Before opening a Git-backed project, inspect Automatically pull and other background source-control defaults. The disposable clone contains any checkout changes or fetches.
4. Prepare nested files with duplicate basenames, Markdown, CSV or TSV, a long line, and a large text file. Add baseline commits and known staged, unstaged, renamed, deleted, and untracked scratch changes.
5. Use available tiny media fixtures for previews. If a format requires an unavailable renderer or permission, record that branch as fixture-blocked. Do not install tools just to manufacture coverage.
6. Use harmless terminal commands that print distinct markers and cwd. Any running-process test uses an audit-owned short-lived process. Track its PID and terminal ID.
7. Use only naturally available read-only PR changes for refresh freshness. Use empty filters, invalid input, or a nonexistent read-only reference for safe error paths. Do not log out, remove credentials, or disconnect user environments.
8. Parent owns all Sol and Sonnet turns, subagents, guides, handoffs, cancellation, and thread closure. No model was run by the checklist author.

Local Ferrite metadata identifies these read-only PR candidates. Remote availability, current permissions, checks, and comments remain unverified by this worker.

| Candidate | Local merge evidence | Use |
| --- | --- | --- |
| [PR #189](https://github.com/josephwylie/ferrite/pull/189) | `265fae71`. Second code review fixes for #162. | Summary, Timeline, Code, links, and merged-state menu inspection. |
| [PR #187](https://github.com/josephwylie/ferrite/pull/187) | `7fe477e2`. The app owns the window's frame. | Summary, Timeline, Code, links, and merged-state menu inspection. |

`origin` is `https://github.com/josephwylie/ferrite.git`. Remote browser branch names in local Git do not establish current open PR numbers or states. Do not infer an open fixture from a branch name. If open-state checks or actions have no available fixture, mark those variants fixture-blocked.

### Use the following order

1. Record the installed version, current preferences, original selected thread, open tabs, and any existing terminal sessions. Prepare the two scratch threads.
2. Run H01 through H06. Resolve wrong-target or unusable-panel behavior before relying on later screenshots.
3. Run P01 through P17 and P22 through P24 against the read-only PR. Inspect drafts and mutation menus under P18 through P21 and P25 through P26.
4. Run Files, Diff, and Terminal cases against known scratch contents. Check the bytes, ranges, cwd, and selected thread after every action.
5. Run the lifecycle and settings cases. Restore each temporary preference before the next scope-sensitive test.
6. Let the parent run L04 and X03 under the actual selected Sol and Sonnet provider instances. Let the parent own any P27, P28, or X02 generation.
7. Run X01 through X06, then finish the cleanup ledger. Reopen the original thread and compare its draft and tabs with the baseline.

### Record each actual action against its source

- Before. Case ID, installed T3 version, selected environment/project/thread or draft, model/provider if applicable, current URL or PR number, panel state, and full-window screenshot.
- Action. Exact visible label, input text without secrets, pointer or keyboard method, and time. Use a normal rendered control. Direct store mutation or helper calls cannot pass a native journey.
- After. Full-window screenshot plus relevant close-up, resulting route and selected thread, active tab, content identity, loading/settled state, and any error. Include actual saved bytes or process state when the assertion needs them.
- Motion. Capture a frame sequence or video for opening, closing, and reversal. Still before/after images only prove end states. Record animation setting and reduced-motion state.
- Source_match. Read the cited symbol before running the case. Record which handler/state matches the observed result. A source-installed-version mismatch stays a separate uncertainty.
- Failure. Repeat once from a known state if safe. Preserve precise steps and targets. Record reproduction count and whether the failure is functional, visual, interaction, provider capability, fixture-blocked, or version mismatch.
- Cleanup. Record every created thread, draft, child, terminal/process, tab, checkout/worktree, project entry, file, and changed preference. Confirm stopped/closed/restored state directly. Closing a tab does not prove a process stopped.

Use the companion JSON `result_template` for each run. Store separate records for different provider, width, input method, and thread-target variants. Keep the checklist status at not-run until the parent adds runtime evidence. Preserve the exact input sequence for a failure.

### Compare provider-specific behavior

| Contract | Required distinction |
| --- | --- |
| `shared-client` | Client-side interaction does not by itself prove any provider workflow. Record selected environment and any capability-gated UI. Run once per relevant layout, with provider repetition only when the observed path depends on provider state. |
| `host-capabilities` | The safe real fixture is GitHub. Do not generalize it to other hosts. sourceControlClients supplies terminology and URL parsing. GitLab uses MR and merge request. PullRequestDetail.capabilities and viewerPermissions decide comments, reviews, edits, viewed storage, stacks, and actions. Server pullRequestChecks gates the checks query. Lack of a permission or capability is fixture-blocked unless the UI misrepresents it. |
| `agent-provider` | Parent may use the authorized Sol and Sonnet subscriptions in T3. Record the actual provider instance, model, effort, account kind without identity or secrets, and child/thread IDs. Do not infer one provider stream from the other. A provider-native child is not necessarily an independently writable chat. |
| `host-and-agent` | Apply both host-capabilities and agent-provider. Record the host PR identity and agent destination. Preparing a worktree, attaching context, and automatically starting a turn are separate actions. |
| `environment-capabilities` | File access, terminal availability, and device or remote execution depend on the selected environment and its granted scopes. A local scratch success does not prove remote behavior. Use existing safe remote fixtures only, otherwise label the remote variant fixture-blocked. |

The provider capability declarations differ in ways that matter to the live-output complaint. Read the declarations before interpreting an absent control.

| Capability in pinned source | Codex provider | Claude provider |
| --- | --- | --- |
| Assistant and reasoning text streaming | Yes. | Yes. |
| Tool output streaming | Yes. | No. Completed tool output is emitted. |
| Native child thread IDs exposed | Yes. | No. |
| Native wait, close, and fork child capabilities | Yes. | No. |
| Subagent lifecycle emitted | Yes. | Yes. |
| Child launch acknowledgement means completed | No. Observe lifecycle. | No. Async launches settle through task notifications. |

- [CodexProviderCapabilitiesV2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255). Codex declares streaming tool output, native subagent thread IDs, wait, close, and fork support. Record the actual installed provider and model before using this contract.
- [ClaudeProviderCapabilitiesV2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489). Claude declares assistant and reasoning streaming, no streaming tool output, and completed tool output. It emits subagent lifecycle but does not expose native child IDs or native wait, close, or fork capabilities.
- [claudeToolResultSettlesSubagent](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L3725). An asynchronous Agent launch acknowledgement does not settle a Claude child. The task notification supplies the completion event.
- [reportSubagentProgress](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L2889). A Codex native child running item updates its parent subagent progress row. Repeated identical progress is not emitted again.

Record the actual provider instance and model behind the Sol and Sonnet selections. Do not assume that a marketing label identifies the adapter. A provider capability difference does not excuse wrong navigation, missing completed content, or an unresponsive control.

### Stop before writes outside the audit fixture

Some menu items mutate the host on the first click. Inspect the rendered menu and its source handler before choosing an action. A confirmation is not universal.

**Host writes that can run immediately.** **Ready for review**, **Convert to draft**, **Reopen pull request**, **Disable auto-merge**, **Allow edits from maintainers**, **Submit review**, **Submit comment**, **Save edited comment**, **Resolve or reopen review thread**, **Reaction**, **Request reviewer**, **Host-backed Viewed toggle**.

**Actions that may be armed and dismissed only after confirming the two-step implementation.** **Merge now**, **Enable auto-merge**, **Close pull request**, **Revert changes**.

**Inspect these controls and cancel before the operation.** **Delete branch**, **Publish repository**, **Create PR**, **Push**, **Pull**, **Commit confirmation**, **Checkout destination**, **Delete thread**, **Delete project**, **Delete now**, **Reset defaults**, **Change update track**, **Install update**, **Create pairing link**, **Reveal secret or webhook endpoint**.

**Agent actions owned by the parent.** **Explain this PR**, **Fix findings**, **Write guide**, **Rewrite guide**, **Regenerate title**, **Automatic commit or PR writing**, **Voice Test model**, **Side chat with a nonempty prompt**.

**Reversible actions limited to owned fixtures.** **Change file bytes**, **Link a PR to an audit thread**, **Rename or pin an audit thread**, **Settle or snooze an audit thread**, **Change presentation preference with recorded original value**, **Open or close an audit terminal**, **Draft review text without submission**.

Closing a menu or tab does not stop an agent, terminal, or scheduled task. Do not manufacture error states by revoking credentials, signing out, stopping a user environment, or changing system trust.

### Inspect visual and interaction quality in every case

- Capture the entire window so panel width, chat width, selected thread, and content relationship remain visible. Add a close-up when a small control fails.
- Record clipped labels, overlapping controls, wrong alignment, unreadable contrast, excessive empty space, oversized defaults, and missing loading or error states. Describe the concrete defect and its affected bounds.
- Test ordinary click, right-click, keyboard focus, Escape, selection, and scroll in the component being exercised. Verify that the event does not reach the composer or a hidden panel.
- Compare open and close motion separately. Test rapid reversal when motion is enabled. Record default zero-duration behavior separately from a broken animation setting.
- For citations, quotes, and attachments, verify the selected source, range, readable preview, user-written note, removal, and persistence through a thread switch.

## Where the cases live

Use these ranges to divide native execution without losing ownership. Every case remains a separate result.

| Range | Scope | Count |
| --- | --- | ---: |
| H01 to H06 | Panel and route behavior. | 6 |
| P01 to P30 | PR workspace, detail, drafts, actions, guides, and errors. | 30 |
| F01 to F09 | Files, previews, editing, and context. | 9 |
| D01 to D07 | Local Diff and Git controls. | 7 |
| T01 to T06 | Terminal input, output, links, layout, and cleanup. | 6 |
| L01 to L12 | Projects, drafts, threads, and worktrees. | 12 |
| S01 to S24 | Settings, schedules, providers, and maintenance. | 24 |
| X01 to X06 | Cross-panel behavior and final persistence. | 6 |

## Check panels and target ownership

### H01. Surface creation, activation, and duplicate avoidance

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Add panel surface / Open a surface → Files, Diff, Terminal, Browser, Pull request

**Fixture.** Scratch thread A; panel initially closed

**Run.** Open each applicable surface from its visible menu. Reopen the same file and PR. Click every resulting tab.

**Assert.** Correct body under the selected title; no stale body from the previous surface; resource reopens activate rather than duplicate; unavailable choices explain why.

**Cleanup.** Close audit-created tabs; preserve any pre-existing tabs.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [pullRequestSurfaceId / RightPanelSurface](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L267). Surface identity includes the PR target. ThreadRightPanelState stores surfaces and activeSurfaceId. Reopening a resource must activate its existing identity.

### H02. Per-thread panel target ownership

Status. **not-run**. Mode. `local-reversible`. Provider contract. `host-capabilities`.

**Entry.** Two scratch thread rows; Add panel surface

**Fixture.** Scratch threads A and B with distinct filenames and terminal text

**Run.** A opens Files and B opens Terminal. With A selected, click the PR badge on B. Repeat with B selected and its own badge. Exercise a linked PR count if available. Alternate threads and open resources again.

**Assert.** The clicked row thread becomes the route when its PR opens in the right panel. Title, project, PR number, selected tab, and displayed body agree after every click. A hidden panel update with no route change fails.

**Cleanup.** Return to original selected thread; close only audit tabs.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [handlePrClick / handlePrListClick](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962). openPrLink receives the clicked row threadRef. If the panel opens on a non-route thread, onThreadActivate changes the route to that thread.

### H03. Width and reopen sequence

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Panel resize edge; Add panel surface; collapse/maximize controls

**Fixture.** Fresh scratch thread at recorded window size

**Run.** Measure closed/open width. Drag compact, open Files→Browser→Diff→PR, collapse/reopen, switch thread and return. Maximize/restore once.

**Assert.** User-chosen width survives appropriate transitions; no tool permanently enlarges the panel without a resize; conversation stays usable; drag follows pointer and finishes on outside release.

**Cleanup.** Restore original size and maximization.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [usePreviewPanelInlineSize / getPreviewPanelMaxWidth](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/usePreviewPanelInlineSize.ts#L20). useResizableWidth persists widthStorageKey. Default width is 540, minimum is 360, and the maximum caps the viewport fraction and remaining chat width.

### H04. Tabs support normal mouse and keyboard interactions

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Tab context menu: Close, Close others, Close to the right, Close all; Scroll tabs left/right

**Fixture.** Only audit-created tabs in scratch thread

**Run.** Overflow the strip with several files; inspect menus; test wheel/trackpad, keyboard focus, close button, and supported middle-click. Run close variants only on audit tabs.

**Assert.** Active tab stays visible; close picks a sensible neighbor; tab close never selects an unrelated resource; focus and keyboard continue to work afterward.

**Cleanup.** Close remaining audit tabs.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [RightPanelTabs.handleTabContextMenu / handleTabAuxClick](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L1180). Context items call close-one, close-others, close-to-right, and close-all callbacks. Middle click calls the close path. Keyboard and pointer activation must agree.

### H05. Animation and reduced-motion contract

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Settings → Appearance → Panel animations; Toggle main sidebar

**Fixture.** Scratch thread; record existing animation preference

**Run.** Capture both left-sidebar and right-panel open and close at the recorded current setting. Set a nonzero local duration if needed to exercise the supported animation path. Reverse transitions rapidly, then restore the original setting. Record reduced-motion preference without changing the OS setting.

**Assert.** Record actual default separately from available motion. No invisible hit target, stuck width, focus in a closed panel, or old content during reversal. Still screenshots cannot prove smoothness.

**Cleanup.** Restore duration and any changed reduced-motion preference; do not alter OS accessibility settings merely for this test.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [usePanelAnimationSettings / usePanelPresence](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L31). active requires a positive duration, no reduced-motion preference, and no navigation suppression. Closing retains the same scope value until the exit duration ends.
- [DEFAULT_PANEL_ANIMATION_DURATION_MS](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/settings.ts#L164). The source default is zero. First record the installed setting. Test positive-duration animation separately from the default.
- [Sidebar panelAnimationsActive](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/sidebar.tsx#L301). Sidebar motion uses the configured duration and reduced-motion preference. Navigation suppression intentionally does not snap a closing sidebar sheet.

### H06. Narrow window and focus routing

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Window edge; panel resize handle; Tab / Escape / app keybindings

**Fixture.** Scratch thread with Files, PR or Terminal open

**Run.** Resize around 980px and 760px source thresholds; traverse controls with keyboard; click composer after panel input and back.

**Assert.** Inline/sheet switch is usable; no clipped action row, overlapping hit regions, lost draft, or shortcut stolen by hidden controls. Installed-version thresholds may differ and must be recorded.

**Cleanup.** Restore window geometry and input focus.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ChatRightPanel](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatRightPanel.tsx#L19). The chat panel passes its measured inline width and sheetAnimationDurationMs into the shared side-panel layout. Verify both inline and sheet modes.

## Check the PR workspace and detail

### P01. Standalone PR workspace is reachable

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Pull requests; Refresh pull requests

**Fixture.** Read-only PR fixture project, initially no selected PR

**Run.** Open the visible Pull requests navigation entry from a thread and from the home state; refresh once.

**Assert.** List is accessible independently of a current branch PR; loading completes to real rows, honest empty state, or actionable error; current project/server scope is visible.

**Cleanup.** Restore prior view and filters.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [SidebarUtilityMenu.handlePullRequestsClick](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarChrome.tsx#L234). The utility menu has an explicit route action for the standalone Pull requests page. Verify the page, selection, and history destination.

### P02. Involvement and lifecycle filters are independent

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** All / Reviewing / Authored; All / Open / Closed / Merged

**Fixture.** Ferrite-origin scratch clone; known merged #189 and #187

**Run.** Switch involvement and state independently. Use Merged to find known references, then restore.

**Assert.** Rows match both filters; selected detail does not silently become a different PR; counts/empty explanation are coherent.

**Cleanup.** Restore prior involvement/state filters.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [INVOLVEMENT_TABS / STATE_TABS](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/pullRequestListOptions.ts#L19). Involvement and state are separate option sets. Each selected filter must change only its own query field.

### P03. Project, server, and host scopes do not leak selection

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Filters → All projects / All servers; host selector

**Fixture.** Scratch project plus existing project only if already available

**Run.** Select scratch project; inspect host/server scope; change filter while a PR detail is open.

**Assert.** List filtering and selected PR identity are explicit; no same-number PR from another repo appears under stale header; unavailable project explains its state.

**Cleanup.** Restore scope; do not connect a new server.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestsPage.scopeKey / usePullRequestScope](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L290). The list scope key includes environment, assignment, state, involvement, project, host, draft, review, checks, author, and labels. Cached results must match that scope.

### P04. Search works for number, text, and label syntax

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Search pull requests; placeholder Search pull requests, or label:bug

**Fixture.** Known #189 and a visible row with a label, if available

**Run.** Search number/title, a visible label: value, then a nonsense token; clear with normal keyboard editing.

**Assert.** Results correspond to query; no-results is recoverable; typing does not trigger global shortcuts; clearing restores rows.

**Cleanup.** Clear audit query.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestSearchInput](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestListFilters.tsx#L96). The input keeps a draft and reports the query through onChange. Test typed text, clear, label syntax, and the resulting list scope.

### P05. Advanced filter combinations remain understandable

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Filters → Drafts only / Hide drafts; Approved / Changes requested / Review required / No reviews; Passing / Failing; Author; Labels

**Fixture.** Available rows; record which facet values exist

**Run.** Combine draft, review and check filters; search authors and labels; remove one filter at a time.

**Assert.** Selected facets remain visible; labels do not collide; empty state says Nothing under these filters; clearing does not discard unrelated filters.

**Cleanup.** Restore every filter changed.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestFiltersMenu / PullRequestAuthorChoices / PullRequestLabelChoices](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestListFilters.tsx#L469). Menu controls represent independent draft, review, checks, author, and label fields. Facet choices and selected labels must survive reopening.

### P06. Sorting states have visible meaning

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Merge readiness / Blocked on me / Recently updated / Newest shown / Oldest shown / Largest shown / Smallest shown

**Fixture.** At least two real PR rows

**Run.** Switch sort options; compare visible dates or change sizes and inspect grouped sections.

**Assert.** Order matches label within available loaded results; a partial result set is not presented as exhaustive; selected row remains identifiable.

**Cleanup.** Restore previous sort.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [SORT_OPTIONS](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/pullRequestListOptions.ts#L32). The list provides explicit sort values. PullRequestsPage uses eager stats for ready, largest, and smallest, so wait for actual ordering before passing.

### P07. List keyboard navigation opens the intended PR

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** PR rows; Search pull requests

**Fixture.** Known merged #189 and #187

**Run.** Use keyboard to move through rows and open, then repeat with pointer; return/back and reopen.

**Assert.** Focus is visible; selected PR number/repo matches the chosen row; no extra click needed; back restores useful list position.

**Cleanup.** Close only newly opened PR tabs.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestsPage.onKeyDown](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1481). The page keyboard handler operates on its displayed entries and current selection. Verify the selected PR identity after navigation, not only the row highlight.

### P08. Multiple PR tabs remain distinct

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** PR row clicks; panel PR tabs; Close others

**Fixture.** Known #189 and #187 in same repo; second repo only if already available

**Run.** Open both; alternate tabs; close one; reopen the same number.

**Assert.** Distinct number/title/target; no duplicated or swapped body; correct tab survives closing; same-target reopen avoids unnecessary duplication.

**Cleanup.** Close audit PR tabs.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestsPage entry selection / openPullRequest](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1125). A list entry opens a PR surface under the standalone page panel ref. Tabs must retain distinct target identities and selected bodies.

### P09. Empty and unavailable projects have useful recovery

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** No projects in this workspace / No pull requests / Nothing under these filters / Pull requests unavailable

**Fixture.** A scratch repo without a remote; nonsense search for empty result

**Run.** Observe no-remote and filtered-empty states without changing live auth; inspect offered navigation/retry links.

**Assert.** Each condition is distinguished; no misleading spinner forever; recovery returns to list without losing user context.

**Cleanup.** Remove filters; leave live auth/config unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestListEmptyState](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestListEmptyState.tsx#L48). Empty rendering distinguishes the lack of projects, filtered-out entries, and no PRs. Recovery must change the cause, not only dismiss the message.

### P10. Summary resolves the exact requested PR

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Summary; PR number/repo header

**Fixture.** Known merged #189, optionally #187

**Run.** Open detail from list. Compare number, repository, title, state, base/head, changed-file count and description to its visible list identity.

**Assert.** Complete target identity; body has readable Markdown/code/links; merged state is settled rather than showing active merge affordance; data is not borrowed from prior tab.

**Cleanup.** Return to list or keep this audit PR for next cases.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [usePullRequestDetail / pullRequestRefKey](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L49). Detail, activity, checks, cached snapshots, and shared summaries resolve under environment plus project, host, repository, and number. Confirm every visible field belongs to that key.

### P11. Checks expand into actionable detail

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Checks / Checks: <summary>; individual check row

**Fixture.** Real PR with at least one check; unavailable checks recorded as fixture gap

**Run.** Open checks popover; inspect name, status, description and any outbound detail link; open one read-only detail destination if available.

**Assert.** Summary matches individual rows; pending/failure/skipped are distinguishable; tooltip is not the only way to read essential data; link activates correct check.

**Cleanup.** Close popover and extra external tab.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [LazyChecksBody / ChecksBody](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx#L30). The checks popover loads detail when opened and keeps showAll state. Confirm individual job identity, status, and outbound target.
- [usePullRequestDetail.supportsChecks](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L63). The separate checks query requires server capability pullRequestChecks. Live refresh runs for an open PR with supported checks. A missing capability is not a proven UI failure.

### P12. Native refresh updates the actual displayed review

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** More pull request actions → Refresh

**Fixture.** Known PR with loaded Summary and Timeline

**Run.** Capture Summary, Timeline, and Code contents. Use Refresh and record loading and settled states. Compare a naturally changed read-only PR if one is available. Record checked-at or query evidence if safely exposed. A spinner alone cannot pass freshness.

**Assert.** Detail and activity refresh under the same PR identity. Existing useful content remains or has clear loading status. Errors are visible. Check whether checks data revalidates through its separate query instead of assuming the Refresh button directly calls checks refresh. Unchanged static data alone cannot prove freshness.

**Cleanup.** Close menu; no host writes.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestDetailPanel.refetchFromHost / refreshDetail](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L665). Explicit refresh invalidates the host cache, refreshes detail, activity, and stack, then increments refreshToken. Observe content or read evidence, not only the spinner.
- [PullRequestCodeTab.appliedRefreshToken](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L552). A new refreshToken resets diff slices, forgets the applied page, and refreshes first-page diff and viewed-file data.

### P13. Timeline preserves discussion structure

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Timeline; Open activity on host; More replies if present

**Fixture.** A PR with comments/reviews/replies; inspect #189/#187 to choose

**Run.** Read chronology, authored timestamps, review verdicts, resolved/outdated threads, replies, snippets and host link. Expand collapsed/reply groups.

**Assert.** Order is coherent; replies remain attached to their thread; resolved/outdated state is visible; code context and Markdown remain readable.

**Cleanup.** Collapse optional expansions if restoring initial state matters.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestTimelineTab / ReviewThreadEvent](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx#L732). Timeline builds activity events and keys review threads by resolution and dismissed state. Inspect grouped replies, collapsed discussions, and exact linked anchors.

### P14. Timeline can attach context without sending

Status. **not-run**. Mode. `local-reversible`. Provider contract. `host-capabilities`.

**Entry.** Timeline → Add to chat

**Fixture.** Scratch thread A and a comment/activity item

**Run.** Use Add to chat, type a distinct audit note beside the resulting context, switch away/back, then remove the attachment.

**Assert.** Attached item retains PR/repo/activity identity and user annotation; composer input remains editable; no prompt is sent automatically.

**Cleanup.** Delete only audit draft text and context attachment.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [AddToChatButton / PullRequestTimelineTab.onAddToChat](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx#L205). The button passes the selected comment into local chat context. The composer must retain the comment target and additional typed text.

### P15. Code tab presents the correct whole change

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Code; Diff scope: All commits

**Fixture.** Known merged PR with multiple changed files

**Run.** Open Code, browse file tree, collapse/expand files, switch between All commits and one visible commit.

**Assert.** Hunks/path/additions/deletions correspond to selected scope; target label updates; unavailable/truncated portions are explicit.

**Cleanup.** Restore All commits and original file selection.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestCodeTab.sliceState / scopeKey](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L424). Diff slices belong to the current PR and commit scope. Changing the scope must not retain files or comments from a previous PR.

### P16. PR diff remains usable at realistic widths

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Diff layout; Stacked diff view / Split diff view; Enable diff line wrapping; Hide test files; Show file tree

**Fixture.** Known PR with long lines and test files if available

**Run.** Toggle each presentation control, drag explorer width with pointer and keyboard, navigate a distant file, then resize panel.

**Assert.** No horizontal content traps or missing line numbers; active file follows selection; wrapping/test filtering affects only intended display.

**Cleanup.** Restore layout/wrapping/tree/test-filter settings.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestCodeTab.foldChoices / wordWrapOverride / fileTreeOpen](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L400). Fold, wrapping, file-tree, hidden-test, and selection state are distinct. Verify content and scroll after each view change.

### P17. Viewed state distinguishes local and host persistence

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `host-capabilities`.

**Entry.** Viewed / Changed; These ticks are kept here, not on the host

**Fixture.** Known read-only PR

**Run.** Inspect viewed controls and locality notice. Do not toggle when it would mark viewed on the host. If explicitly local-only, toggle one file, navigate away/back, then restore.

**Assert.** A local-only tick is honestly labelled; changed-since-viewed is distinct; hidden files do not misstate completion.

**Cleanup.** Undo local tick; no host viewed-state mutation.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestCodeTab.viewedFilesStore / usePullRequestFilesViewed](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L539). Viewed-file storage is a capability value. A host-backed toggle can write remotely. Only exercise a positively identified local-only viewed store during the read-only pass.

### P18. Review draft supports text, verdict and preview without submission

Status. **not-run**. Mode. `draft-only`. Provider contract. `host-capabilities`.

**Entry.** Review summary; Review verdict → Comment / Approve / Request changes; Submit review

**Fixture.** Known PR where review form is offered; scratch context

**Run.** Enter a clearly synthetic draft, switch offered verdicts, inspect validation and Markdown preview if present. Navigate within PR and return. Never press submit shortcut or Submit review.

**Assert.** Draft text survives harmless tab changes; required summary rule is clear; disabled submit is understandable; permission/self-review restriction is explicit.

**Cleanup.** Clear audit review draft and inline drafts; verify no posted activity.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestReviewForm / usePullRequestReviewStore](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestReviewForm.tsx#L52). summaries and drafts are keyed by pullRequestReviewKey. requestedVerdict is local. submit calls pullRequestEnvironment.submitReview and then clears submitted draft data only after success.

### P19. Inline review comments retain range and typed note

Status. **not-run**. Mode. `draft-only`. Provider contract. `host-capabilities`.

**Entry.** Code line selection / comment affordance; Add to agent

**Fixture.** Known PR with text diff

**Run.** Select a single changed line and multi-line range; open comment editor; type a note; inspect add-to-review vs agent context actions. Use only local draft action.

**Assert.** Correct path and side/range are retained; quote/context and typed text coexist; switching files does not silently drop local draft.

**Cleanup.** Remove audit inline drafts and composer attachments; never publish.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestCodeTab.canCommentOnLines / canSelectLines / rangeComments](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L591). Inline host comments require capability and permission and only work on the whole-change scope. Agent context is local and can remain available without host comment permission.

### P20. Comment composer is readable and cancel-safe

Status. **not-run**. Mode. `draft-only`. Provider contract. `host-capabilities`.

**Entry.** Leave a comment; Write / Preview; comment submit control

**Fixture.** Known PR with comment permission

**Run.** Type synthetic Markdown with a link and code; switch Write/Preview; inspect mention completion if offered. Do not submit or use submit keyboard chord.

**Assert.** Preview corresponds to text; focus returns to editor; no submission on ordinary Enter; input state persists only as expected.

**Cleanup.** Clear text and close editor; confirm Timeline unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestCommentForm.body / submitting](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCommentForm.tsx#L20). Comment body is local until the submit handler calls the host. The submitting mode distinguishes comment, close, and reopen.
- [PullRequestMarkdownEditor.draft / preview / seed](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestMarkdownEditor.tsx#L19). Write and Preview share the draft. Changing previews must not drop text or trigger submission.

### P21. Edit/reply/resolve controls do not masquerade as local UI

Status. **not-run**. Mode. `inspect-only`. Provider contract. `host-capabilities`.

**Entry.** Edit comment; reply field; Resolve thread / reopen control if offered

**Fixture.** Existing PR activity, preferably own comment only if one already exists

**Run.** Inspect available controls, permission hints and draft editors. Do not save, react, resolve, reopen, or edit someone else's content.

**Assert.** Controls are scoped to correct item; edit/reply drafts have visible cancel; unavailable operations are explained rather than dead links.

**Cleanup.** Cancel editors; verify no new or changed activity.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ConversationCard.editing / saving](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx#L230). Editing and saving are separate states. Comment edits, reactions, and thread resolution cross the host mutation boundary.

### P22. PR links support normal browser/context behavior

Status. **not-run**. Mode. `local-reversible`. Provider contract. `host-capabilities`.

**Entry.** PR context menu: Copy link; Open on GitHub. Markdown HTTP link: Open in integrated browser; Open in system browser; Copy Link.

**Fixture.** Known merged PR and scratch composer containing its URL

**Run.** Right-click the PR badge and a normal Markdown HTTP link. Select each supported read-only open action once and verify the actual URL. Copy the link and inspect its value without exposing unrelated clipboard history. Test modifier click separately.

**Assert.** Normal click opens intended native target when configured; external path remains available; copy result matches exact URL/number; right-click does more than inert underline.

**Cleanup.** Close external audit tab; restore preference and clipboard if needed.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestActionsMenu.openOnHostText / copyReference](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L155). The menu separates open-on-host from Copy link and Copy reference. Verify each destination and the native context menu independently.
- [showPullRequestLinkContextMenu](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts#L26). The PR context menu offers Copy link and Open on the host. The open-external branch calls api.shell.openExternal with the clicked URL.
- [showExternalLinkContextMenu](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/externalLinkContextMenu.ts#L88). HTTP links offer Open in integrated browser when available, Open in system browser, and Copy Link. Each action calls a distinct handler.

### P23. Link PR dialog validates and targets the chosen thread

Status. **not-run**. Mode. `local-reversible`. Provider contract. `host-capabilities`.

**Entry.** Link pull request; Pull request URL or #42; Choose a thread

**Fixture.** Scratch thread only; known #189 URL

**Run.** Open Link pull request; try malformed text then valid full URL; inspect target repo/number. Link only to scratch thread if supported, switch threads, and inspect linked indicator.

**Assert.** Malformed input produces inline validation; valid absolute URL avoids ambiguous repo; linked badge reveals correct native PR from another selected thread.

**Cleanup.** Unlink scratch association if offered or remove scratch thread during cleanup; never link existing user thread.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [LinkPullRequestDialog.resolved / dirty / submitError / pending](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/LinkPullRequestDialog.tsx#L89). Validation resolves URL or number against the target thread project. The linking command must use that threadRef and environment.

### P24. Linked threads and stacks navigate correctly

Status. **not-run**. Mode. `read-only`. Provider contract. `host-capabilities`.

**Entry.** Linked pull requests; linked-thread count; Stack <n>, layer <n>; Retry stack refresh

**Fixture.** Scratch linked PR; real stack only if already present

**Run.** Open linked PR collection, navigate linked thread, inspect stack layer/menu if available.

**Assert.** Links preserve selected repo and environment; layer navigation updates detail; unavailable stack explains retry without dumping the user elsewhere.

**Cleanup.** Close audit linked panels; restore selection.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [EnabledPullRequestThreadLinks.changeLink / linkedThreads](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestThreadLinks.tsx#L92). Relations are environment-scoped and filtered to visible threads. Link changes are local T3 relations, while navigation must target the selected related thread.
- [PullRequestDetailPanel.supportsStackActions / stackOwnsMerge](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L606). Stacks require server support and detail capabilities stacks and stackActions. Native stack membership can own the merge route.

### P25. Mutation menus expose honest state and cancel correctly

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `host-capabilities`.

**Entry.** More pull request actions; Merge now → Confirm merge?; Close pull request → Confirm close?; Revert changes → Confirm revert?

**Fixture.** Real PR appropriate to state; no mutation authorization

**Run.** Inspect menu first. Only where source and visible UI both show a two-step arm, arm once and dismiss with Escape/outside click. Reopen to verify disarmed state.

**Assert.** Correct PR target remains visible; arm is explicit and resets on dismissal/navigation; no mutation occurs. Ready for review, Convert to draft, Reopen, Disable auto-merge and Allow edits from maintainers are immediate writes: do not click.

**Cleanup.** Dismiss all menus; verify original state unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestActionsMenu.inlineConfirm / pendingAction / saveMaintainerEdits](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L159). Some actions arm an inline confirmation. Maintainer edits issue update immediately. Inspect each handler boundary before clicking an enabled action.

### P26. Checkout choices disclose their destination

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `host-capabilities`.

**Entry.** Check out → In a separate worktree / In this repository

**Fixture.** Read-only PR fixture project

**Run.** Open the checkout menu and inspect environment and destination choices. Dismiss before checkout. An optional actual checkout is a separate parent-owned local-reversible test restricted to the disposable clone.

**Assert.** Separate worktree and current repository are visibly different; pending/error state has a clear destination; no silent change to source checkout.

**Cleanup.** Cancel menu; if parent explicitly runs scratch checkout, track worktree path and remove only after thread closure.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestCheckoutMenu.onCheckout](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailActions.tsx#L75). The worktree and local choices call onCheckout with different modes. The menu is disabled without checkoutRoot or while a handoff is active.

### P27. Agent handoffs preserve PR context

Status. **not-run**. Mode. `parent-agent`. Provider contract. `host-and-agent`.

**Entry.** Ask a question; Explain this PR; Fix findings; Write guide / Rewrite guide

**Fixture.** Scratch thread; parent-controlled Sol session only

**Run.** Inspect labels first. Ask a question may attach context; verify behavior before typing. Explain/Fix/Write guide can start an agent immediately and must be exercised only by parent with chosen provider.

**Assert.** Handoff names exact repo/PR; correct scratch thread opens; live progress and cancellation are visible; no publishing/forge writes requested.

**Cleanup.** Stop and close any parent-created agent/thread; remove audit attachments.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [usePullRequestHandoffs / writeHandoffToComposer](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L266). Handoff state prevents duplicate preparation. Composer context and automatic agent starts are separate outcomes. Parent owns any model run and thread cleanup.

### P28. Guide empty, running, stopped and stale states

Status. **not-run**. Mode. `parent-agent`. Provider contract. `host-and-agent`.

**Entry.** Guide; Write guide / Rewrite; Rewrite

**Fixture.** Existing guide preferred; parent-controlled generation if required

**Run.** Read an existing guide and navigate its stops/excerpts. Inspect empty-state writer choice. Parent may run one bounded guide only under intended subscription/provider.

**Assert.** Guide stays tied to PR head; stop selection reveals correct files/ranges; running and failed states are visible; stale guide is marked after changed head if safely observable.

**Cleanup.** Cancel active run; close generated scratch thread; do not leave background generation.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestGuideTab](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestGuideTab.tsx#L22). Guide content, loading, failure, and running state come from the guide query and run state. Inspect available states and let the parent own any generation.
- [startGuideRunNotifications](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/guide/PullRequestGuideRunCoordinator.tsx#L27). The coordinator tracks running watches and handles guide updates. Closing the tab must not be treated as proof that generation stopped.

### P29. PR list/detail errors recover without losing usable content

Status. **not-run**. Mode. `conditional-read-only`. Provider contract. `host-capabilities`.

**Entry.** Pull requests unavailable; Refresh; More pull request actions → Refresh

**Fixture.** Naturally disconnected/unavailable project or read-only nonexistent PR URL in scratch context

**Run.** Use malformed Link PR input for client validation; use nonexistent reference only for read-only missing-resource behavior. Never revoke credentials or disconnect user environment to manufacture failure.

**Assert.** Error is specific, wrapped and actionable; previously loaded list can remain with stale notice; retry works if prerequisite restored; wrong-repo errors never show stale previous PR as current.

**Cleanup.** Remove scratch invalid reference; restore selection.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestsPage.loaded / invalidating / hostReads](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L433). The workspace retains loaded snapshots through refresh and host read errors. Check the visible stale-data notice and whether old rows remain usable.
- [PullRequestsUnavailableState](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsUnavailableState.tsx#L8). The unavailable component receives the error and recovery action. Exercise only a safe missing reference or naturally unavailable connection.

### P30. Diff/activity failures have local retry and external escape

Status. **not-run**. Mode. `conditional-read-only`. Provider contract. `host-capabilities`.

**Entry.** Could not load pull request activity → Retry; Try again; Open on GitHub; Technical details

**Fixture.** Natural missing/oversized/unavailable content only, otherwise fixture-blocked

**Run.** If error occurs, expand technical details and use retry; open the PR externally read-only; resize panel.

**Assert.** Summary, cause, remedy, and long details remain readable; retry is actionable; no clipped line with hidden remedy; already loaded content is not incorrectly cleared elsewhere.

**Cleanup.** Close details and extra external tab.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PullRequestActivityUnavailableState](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActivityUnavailableState.tsx#L7). Activity failure has its own retry. Detail loading success does not establish Timeline success.
- [PullRequestDiffUnavailableState](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDiffUnavailableState.tsx#L11). Diff failure exposes a retry, an external target, and technical details. Test text wrapping and target preservation.

## Check files and context

### F01. Files entry and tree navigation

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** Add panel surface → Files; Search files; Refresh workspace files

**Fixture.** Scratch tree with nested folders and duplicate filenames

**Run.** Open tree, expand/collapse nested folders with pointer and arrows, search relative name/path, clear, refresh.

**Assert.** No blank tree after refresh; indentation and selection are clear; duplicate basenames remain distinguishable; keyboard expands a folder without accidental file open.

**Cleanup.** Clear search and restore expansion if needed.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [FileBrowserPanel.handleRefresh / query / expansionRoot](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FileBrowserPanel.tsx#L420). File tree search and expansion use environment plus cwd. Refresh must update the directory data and selected file without navigating to another checkout.

### F02. Open files and navigate retained breadcrumbs

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** File row; File contents; breadcrumb folders

**Fixture.** Scratch nested src/sample.ts and docs/sample.ts

**Run.** Open both files; traverse parent breadcrumbs and a retained child; click file links with line targets when available.

**Assert.** Correct full path/content shown; repeated opens do not duplicate unnecessarily; line target is visible and highlighted, not only scrolled somewhere nearby.

**Cleanup.** Close file tabs.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [FilePreviewPanel / useFileLineReveal](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1321). relativePath, revealLine, and revealRequestId identify the requested file and location. Same basenames and repeated line opens must retain correct context.

### F03. Large file and long-line behavior

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** Enable word wrap / Disable word wrap; source view

**Fixture.** Scratch long-lines.txt and a large generated text file; no private files

**Run.** Open each, scroll rapidly, select/copy near top and lower region, resize narrow/wide, toggle wrapping.

**Assert.** No missing text, jumps to wrong position, unusable horizontal trap, or misleading truncation; any first-1MB limit is explicit.

**Cleanup.** Close files; restore wrapping.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [WorkspaceLargeTextPreview](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1738). Large text uses a separate preview with saving, revision, and loaded URL state. Verify the supported limit and recovery from a failed load.

### F04. Markdown and delimited data have useful alternate views

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** Rendered/source toggle; Full file; CSV table view

**Fixture.** Scratch Markdown with table/code/link/task-list and small CSV/TSV

**Run.** Switch rendered/source views; follow a local link; test table scroll and copy; inspect Markdown task checkbox without editing source user data.

**Assert.** Rendering and source agree; tables fit/scroll; external/local links keep their meaning; toggles do not lose selected file or scroll unexpectedly.

**Cleanup.** Restore display preference; any checkbox edit only in scratch and undo afterward.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [FilePreviewPanel.renderMarkdownPreferred / renderTablePreferred](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1182). Markdown and table view choices persist separately. Switching representation must keep the file target and text intact.

### F05. Media/document previews have finished loading/error states

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** Files → image, PDF, audio/video, office/font/binary preview as fixture permits

**Fixture.** Public/synthetic local files only; representative files created by parent if available

**Run.** Open representative supported types, zoom/pan/page/play where visible, then malformed file and missing path.

**Assert.** Correct preview vs unsupported explanation; text does not spill; loading/error controls remain reachable; unsupported does not appear as blank.

**Cleanup.** Pause media; close viewers; no external upload.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [WorkspaceDocumentPreview / WorkspaceImagePreview / WorkspaceVideoPreview / WorkspaceAudioPreview](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L401). Each media or document renderer handles a resolved resource. Verify supported content, failure state, and width on the installed platform.

### F06. Local file editing saves the intended file

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** File contents editor; normal save chord; pending/unsaved indicator

**Fixture.** Scratch text file with known marker only

**Run.** Edit one marker, save, switch file and back, reopen after tab close. Then make unsaved edit and attempt close to inspect dismissal choices.

**Assert.** Saved bytes appear on reread; unsaved/pending/error status is visible; close behavior preserves or explicitly discards draft; editing never leaks into composer.

**Cleanup.** Restore scratch marker or discard audit draft; close file.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [EditableFileSurface.saveTarget / handleEditChange](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L765). Edits enter useFileSaveSession using environment, cwd, and relativePath. Confirm the scratch file bytes after save and after an unsaved-close decision.

### F07. File context and dragging attach usable context

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** File context → Copy mention / Add to chat; drag file into composer

**Fixture.** Scratch thread and selected file

**Run.** Copy mention; Add to chat; drag file into composer with an existing typed note; inspect attachment then remove.

**Assert.** Correct path and range/contents policy visible; note survives; drag highlight ends after drop/cancel; attachment does not consume whole composer.

**Cleanup.** Remove audit text and attachments; restore clipboard if needed.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [FileBrowserPanel.showEntryContextMenu / dragMention](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FileBrowserPanel.tsx#L192). Copy mention and Add to chat serialize panelPath. Drag uses createFileTreeDragMentionController. Check the inserted target and note without sending.

### F08. File selection comments retain typed text

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** File range selection; comment/context affordance

**Fixture.** Scratch editable/read-only source preview

**Run.** Select line and range; enter annotation; attach via offered local review/context action; switch file and return.

**Assert.** Range, file, quote and added text survive; editing annotation is possible; keyboard selection and context menu behave normally.

**Cleanup.** Remove audit annotations and composer context.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [EditableFileSurface.submitAnnotationEntry / lineAnnotations](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L831). The selection annotation preserves the entry range and submits the typed text through the composer context path. Test cancellation and two selections.

### F09. Explorer resizing and external open are predictable

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** Show file explorer / Hide file explorer; Resize file explorer; Open in; Open file in preview browser

**Fixture.** Scratch HTML and text file

**Run.** Resize explorer with mouse then arrows/Home/End; toggle explorer; inspect/open local file externally or preview HTML as appropriate.

**Assert.** Tree/content keep usable widths; width persists sensibly; correct file opens; closing preview restores useful focus.

**Cleanup.** Close external audit window/tab; restore explorer width and visibility.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [FileExplorerPane](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FileExplorerPane.tsx#L16). The explorer width has responsive limits and resize handlers. Verify pointer and keyboard adjustments plus the preview body width.
- [FilePreviewPanel.handleOpenInBrowser](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1360). HTML-style preview follows a separate browser action. Opening it must preserve file identity and existing editor state.

## Check local changes and Git controls

### D01. Local Changes/Diff exposes known staged and unstaged changes

Status. **not-run**. Mode. `read-only`. Provider contract. `environment-capabilities`.

**Entry.** Add panel surface → Diff; Diff scope selector

**Fixture.** Scratch Git repo with committed baseline, staged addition, unstaged edit, rename/delete, untracked file

**Run.** Open Diff; select each offered scope; compare file counts and hunks with known fixture.

**Assert.** No missed new/deleted file; staged and unstaged distinctions are honest; scope label reflects actual comparison.

**Cleanup.** Restore prior scope; keep fixture changes for later cases.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [DiffPanel.branchDiffPreview / selectedGitSource](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L400). The source of rendered changes depends on thread route, cwd, diffSelection, and selectedGitSource. Compare each known local change to its actual row and patch.

### D02. Base comparison and commit scope remain explicit

Status. **not-run**. Mode. `read-only`. Provider contract. `environment-capabilities`.

**Entry.** Change comparison target. Currently <ref>; Search refs; All commits or offered scope

**Fixture.** Scratch branches main and audit-change with local commits

**Run.** Change base to known local ref; inspect remote-only choice without using network checkout; select commit scope.

**Assert.** Base/head shown correctly; no stale diff from previous ref; unavailable comparison has actionable error, not empty success.

**Cleanup.** Restore original base/scope.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [DiffPanel.selectedBaseRef / selectedCommitSha](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L335). Base ref and commit scope derive from diffSelection. Verify the header, list, and patch all change together.

### D03. Diff presentation controls work with real content

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** Refresh diff; Expand all files / Collapse all files; Stacked diff view / Split diff view; Hide test files; Show file tree

**Fixture.** Scratch diff with long lines and test file

**Run.** Refresh after parent edits scratch file; toggle presentation controls, select tree rows, copy path and code.

**Assert.** New contents arrive; selection stays sensible; wrap/whitespace/test filtering is consistent; colors and line numbers remain readable.

**Cleanup.** Restore presentation settings.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [DiffPanel.refreshDiffFromUserAction / fileTreeOpen / wordWrapOverride](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L477). Explicit refresh invokes the preview query. Presentation controls must preserve the current source and selected file.

### D04. Reviewing a diff can attach context without sending

Status. **not-run**. Mode. `local-reversible`. Provider contract. `environment-capabilities`.

**Entry.** Diff line/range selection; comment/add-to-chat action

**Fixture.** Scratch diff and composer with typed annotation

**Run.** Select changed lines, add local review/context note, change file and return.

**Assert.** Correct before/after side and line range; note remains editable; composer text is not replaced; no agent prompt is sent without explicit send.

**Cleanup.** Clear audit review annotations and draft attachments.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [DiffPanel.reviewSectionId / reviewSectionTitle](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L1467). Review context is scoped to branch, commit, turn, or tool call. The added context must include the right scope, line range, and typed note.

### D05. Git actions disclose exactly what would be changed

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `environment-capabilities`.

**Entry.** Git actions / Git action options; Commit, Push, Pull, Commit & push, Push & create PR, Publish repository

**Fixture.** Scratch repo; active user repo inspection only

**Run.** Inspect available actions and disabled reasons for clean/dirty/no-remote states. Open a form only if it does not execute. Do not click one-shot Pull/Push/combined actions.

**Assert.** Label reflects current state; file selection and target branch are clear; generated text is not assumed free/no-agent; errors have useful context.

**Cleanup.** Cancel opened form; leave repository unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [buildMenuItems / resolveQuickAction](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/GitActionsControl.logic.ts#L109). Git status decides offered actions and the primary action label. Labels can combine commit, push, and PR creation. Do not activate a combined write to inspect it.

### D06. Commit form is complete and cancel-safe

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `environment-capabilities`.

**Entry.** Commit message placeholder Leave empty to auto-generate; file list; Open <file> in editor

**Fixture.** Scratch dirty repo only

**Run.** Open commit UI if source/visible control is a form; inspect file include/exclude, type manual message, open one file, then cancel. Do not execute commit or generation in this pass.

**Assert.** Form distinguishes selected files/staged changes; cancel leaves original working tree/index and message policy understood; no accidental commit from Enter in field.

**Cleanup.** Cancel; discard form draft if possible; record before/after local status.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [GitActionsControl.isCommitDialogOpen / dialogCommitMessage / excludedFiles](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/GitActionsControl.tsx#L995). Commit dialog text and file exclusions are draft UI. Confirming can write Git data and an empty message can invoke generation.

### D07. Publishing and branch-changing dialogs are inspectable

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `environment-capabilities`.

**Entry.** Publish repository; Private / Public; remote-name input; Branch selector → Search refs...

**Fixture.** Scratch repo without remote for publish form; scratch branch picker

**Run.** Inspect repository name/owner/visibility and branch/worktree options; cancel every publishing/create/delete action.

**Assert.** Destination and visibility are explicit; missing permission and name validation are readable; no silent publish or checkout occurs during menu inspection.

**Cleanup.** Cancel; no network writes or user checkout mutation.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [PublishRepositoryDialog / pendingDefaultBranchAction](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/GitActionsControl.tsx#L447). Publishing has provider, repository, visibility, remote, protocol, and step state. Inspect these fields and dismiss before the write.

## Check terminals

### T01. Terminal opens in the correct checkout

Status. **not-run**. Mode. `local-process`. Provider contract. `environment-capabilities`.

**Entry.** Add panel surface → Terminal; New Terminal

**Fixture.** Scratch thread A in known scratch repo

**Run.** Open terminal; run pwd and a simple deterministic printf marker; open Files/PR and return.

**Assert.** cwd matches thread checkout, output arrives live, terminal is not recreated or replaced by Background view on return.

**Cleanup.** Run exit in audit shell when done.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [useThreadTerminalActions / terminalEnvironment.open](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/terminal/useThreadTerminalActions.ts#L28). The terminal opens with its explicit launch location and scoped threadRef. A printed cwd is the runtime assertion.

### T02. Input focus and ordinary terminal keys work

Status. **not-run**. Mode. `local-process`. Provider contract. `environment-capabilities`.

**Entry.** Terminal viewport; composer; app shortcuts

**Fixture.** Scratch shell with no user job

**Run.** Type a multiline quoted command, use arrows/Home/End/backspace, copy/paste, Ctrl-C on a harmless foreground sleep; alternate composer and terminal focus.

**Assert.** Keys go to the visible focused surface; terminal control keys do not send chat; app navigation does not leave terminal focused invisibly.

**Cleanup.** Cancel foreground command; clear scratch composer text.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [TerminalSessionViewport](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L495). Input and session output pass through the active terminal viewport. Verify focus and rendered output without accidentally typing into the composer.

### T03. Horizontal/vertical splits have independent sessions

Status. **not-run**. Mode. `local-process`. Provider contract. `environment-capabilities`.

**Entry.** Split Terminal Horizontally / Split Terminal Vertically; New Terminal

**Fixture.** Scratch thread; short marker per shell

**Run.** Create two split panes then a separate terminal; print distinct markers; switch active panes/groups; resize panel and test split limit if cheap.

**Assert.** Input/output stays with active pane; Single / Stacked / Side by side grouping matches geometry; limit disabled reason visible; no mixed streams.

**Cleanup.** Exit and close every audit shell/pane.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ThreadTerminalDrawer.onSplitTerminalAction / onSplitTerminalVerticalAction](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L1596). Horizontal and vertical split commands choose different layouts. Independent terminal markers must remain in their own session.

### T04. Terminal context menu adds meaningful text

Status. **not-run**. Mode. `local-process`. Provider contract. `environment-capabilities`.

**Entry.** Add to chat / Copy / Paste / Select all / Jump to latest

**Fixture.** Scratch shell output with distinct first/last lines

**Run.** Select output, right-click, copy and Add to chat; keep an existing composer note; scroll up then Jump to latest.

**Assert.** Copied/attached text matches selection, not full private buffer; terminal origin/context remains identifiable; annotation survives; jump reaches current output.

**Cleanup.** Clear audit attachment/note; restore clipboard if needed.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [TerminalSessionViewport.handleAddTerminalContext](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L583). Selected terminal output becomes TerminalContextSelection. Confirm the copied or attached selection and retained composer note.

### T05. Terminal links use the correct target and line

Status. **not-run**. Mode. `local-process`. Provider contract. `environment-capabilities`.

**Entry.** Terminal path:line and http URL link

**Fixture.** Scratch output containing absolute file path:line and controlled localhost URL

**Run.** Click and modifier/right-click supported links; close resulting preview; return to terminal.

**Assert.** File goes to exact line; URL opens intended browser/external destination; trailing punctuation is not included; focus restoration works.

**Cleanup.** Close audit preview/external tab; stop any parent fixture server.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [TerminalSessionViewport.linkPath](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L522). Detected file links and external URLs route through different open paths. Confirm exact file line and browser URL after the click.

### T06. Close, reconnect, thread switch, and restart do not orphan work

Status. **not-run**. Mode. `local-process`. Provider contract. `environment-capabilities`.

**Entry.** Close Terminal / Close <label>; terminal tab close

**Fixture.** Audit shell with harmless foreground process; second scratch thread

**Run.** Attempt close and inspect running-process confirmation; cancel once, then stop the job and close. Switch threads while output is producing. Restart persistence only if parent already plans app restart.

**Assert.** Cancel preserves session; confirmed close actually ends owned process; hidden thread output remains correctly scoped; restored terminal status is honest.

**Cleanup.** Exit/terminate every audit process and verify no live job or terminal remains.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [useThreadTerminalActions.closeTerminalSession](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/terminal/useThreadTerminalActions.ts#L118). The close command is sent for the selected terminal and target thread. A closed tab alone does not prove the process ended.
- [confirmTerminalClose](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/terminalCloseConfirm.ts#L9). Closing a live session can require confirmation. Cancel and confirm are distinct tests. Only confirm for audit-created terminal processes.

## Check project and thread lifecycle

### L01. Add existing scratch folder uses a clear project target

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Add project → Local folder / Add existing project

**Fixture.** Prepared scratch project path

**Run.** Use visible picker to add only scratch folder; inspect selected environment, path, name; open project.

**Assert.** Project appears once in sidebar, correct root displayed; no accidental clone/publish; duplicate add focuses existing project rather than duplicating.

**Cleanup.** Remove/archive only scratch project after threads/processes close.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [OpenCommandPaletteDialog.handleAddProject](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/CommandPalette.tsx#L1997). Adding a folder resolves the chosen environment and cwd. Verify the scratch root and project identity after creation.

### L02. New project and clone flows expose destinations

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `shared-client`.

**Entry.** New project; Add project remote input; Host for the new repository; Select where to clone

**Fixture.** No actual remote clone required; scratch parent directory

**Run.** Inspect local vs hosted project mode, names and destination fields, and invalid URL/name validation. Cancel before create/clone/publish.

**Assert.** Local creation distinct from hosted repository creation; environment/root visible; errors stay near field; Escape and back preserve expected state.

**Cleanup.** Cancel modal and clear unsaved form.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [OpenCommandPaletteDialog.newProjectFlow / addProjectCloneFlow / newProjectPublishes](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/CommandPalette.tsx#L529). New folder, clone, and publish use separate flow state. Inspect destination and selected host before dismissing.

### L03. Draft thread retains text and chosen scope

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** New thread in... / New thread without a project; New thread in <project>

**Fixture.** Scratch project A and optional No project draft

**Run.** Create unsent draft, type marker, switch project/thread and return; change branch/environment picker only where safe.

**Assert.** Draft text and chosen scope remain paired; no provider starts merely from typing; draft indicator and Discard draft work.

**Cleanup.** Discard audit draft after verifying confirmation semantics.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [createNewThreadHandler / seedPrompt](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useHandleNewThread.ts#L87). Draft reuse checks existing user content and preserves it across awaits. Workspace, model selection, and runtime mode are separate context values.

### L04. Sent scratch threads have a complete lifecycle

Status. **not-run**. Mode. `parent-agent`. Provider contract. `agent-provider`.

**Entry.** New thread; provider/model picker; send/stop controls

**Fixture.** Two scratch threads. Parent selects the authorized Sol and Sonnet subscriptions and records actual provider instance and model for each.

**Run.** Parent sends one bounded harmless prompt, observes live output, stops a second bounded run, resumes/follows up only as needed.

**Assert.** Provider/effort/status are visible; stop is real; thread stays selected; output is complete and composer usable afterward.

**Cleanup.** Stop and close every audit thread/run; no replacement-model leftovers.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ChatView.pendingBackgroundTasks / serverProjection](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatView.tsx#L1492). Actual sent turns appear in the projection, including pending background work. Parent must observe start, output, completion, and cleanup under the selected provider.
- [CodexProviderCapabilitiesV2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255). Codex declares streaming tool output, native subagent thread IDs, wait, close, and fork support. Record the actual installed provider and model before using this contract.
- [ClaudeProviderCapabilitiesV2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489). Claude declares assistant and reasoning streaming, no streaming tool output, and completed tool output. It emits subagent lifecycle but does not expose native child IDs or native wait, close, or fork capabilities.

Run and record these provider variants separately.

- Authorized Sol selection mapped to the observed Codex provider instance.
- Authorized Sonnet selection mapped to the observed Claude provider instance.

### L05. Thread rename and title input behave naturally

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Thread title; Rename thread

**Fixture.** Scratch thread

**Run.** Rename via context menu; test empty input, Escape cancellation, Enter commit, and selection/editing; navigate away/back.

**Assert.** Empty title rejected without losing old title; Escape restores old value; committed title appears in sidebar/header/search.

**Cleanup.** Restore audit title or remove scratch thread.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ThreadRenameInput.handleKeyDown](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L602). Rename input has commit and cancel callbacks. Verify saved title and Escape cancellation without starting a new turn.

### L06. Pin, settle, snooze, wake, and unread are reversible

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Pin thread / Unpin thread; Settle thread / Un-settle thread; Snooze → Custom…; Wake thread; Mark unread

**Fixture.** Idle scratch thread only

**Run.** Exercise pin/unpin; settle/unsettle; inspect custom snooze and cancel, or short snooze then immediate Wake; mark unread then revisit.

**Assert.** Row moves predictably; no lost selection/draft; Undo notice works where offered; snooze wake state and unread indicator clear appropriately.

**Cleanup.** Restore original scratch state; cancel scheduled snooze.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [Sidebar.handleThreadContextMenu](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2608). Pin, settle, snooze, wake, and unread act on the clicked ScopedThreadRef. Verify row placement and status after each reversible action.

### L07. Thread search and project grouping preserve intended navigation

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Search threads; Search projects; All projects; Show all projects / Filter by <project>

**Fixture.** Two scratch threads with distinct titles and markers

**Run.** Search partial title/content where supported; switch grouping/filter; use keyboard and pointer to open result; clear search.

**Assert.** Correct thread activates; result text and focus are visible; clearing search restores list without taking user to Background work.

**Cleanup.** Clear audit searches and restore grouping/filter.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [Sidebar.handleThreadSearchKeyDown / threadSearchQuery](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1522). Search highlighting, filtered entries, and activation use their own state. Verify keyboard and pointer selection after grouping changes.

### L08. Thread context copies and branch reuse are scoped

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Copy → Path / Branch / Thread / Thread ID; New thread on <branch>; Project settings

**Fixture.** Scratch thread with branch

**Run.** Copy each value and inspect; open new thread on same branch without sending; open project settings from row.

**Assert.** Copied identifiers correspond to clicked thread, not selected neighbor; new draft inherits intended branch/project; settings scope is correct.

**Cleanup.** Discard extra draft; restore clipboard if needed.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [buildThreadActionMenuItems](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/threadActionMenu.logic.ts#L116). Context actions depend on the thread branch and project. Copy actions and New thread on branch must reference the clicked row.

### L09. Archive and deletion affordances disclose consequences

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `shared-client`.

**Entry.** Archive thread; Settings → Archive → Unarchive / Delete; Delete

**Fixture.** Idle scratch thread; confirmation preference recorded

**Run.** Inspect archive/delete confirmation without committing deletion. If parent separately permits reversible scratch archive, archive then Unarchive and inspect restoration. Never archive/delete user thread.

**Assert.** Running thread action disabled appropriately; confirmation identifies exact thread; archive restore preserves content/context; delete is visually distinct.

**Cleanup.** Cancel deletion; restore scratch archive state.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ArchivedThreadsPanel.handleThreadAction / pendingKeys](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ArchivedThreadsPanel.tsx#L150). Archive actions track pending thread keys and distinguish unarchive from delete. Inspect deletion then cancel. Restore only audit-created archived threads.

### L10. Multi-selection never targets hidden/unselected threads

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `shared-client`.

**Entry.** Sidebar range/modifier selection; Settle (<count>), Snooze (<count>), Mark unread (<count>), Delete (<count>)

**Fixture.** Two audit-only rows plus a user row left untouched

**Run.** Select audit rows, inspect batch menu count, change selection with modifiers and cancel; optionally reversible Mark unread only.

**Assert.** Count and highlight agree; hidden filters do not broaden target unexpectedly; context click preserves intended selection; Escape clears.

**Cleanup.** Clear selection; leave user rows untouched.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [Sidebar.handleMultiSelectContextMenu](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2306). Batch actions receive the selected thread set. Compare menu counts and actual selected IDs before a reversible scratch-only action.

### L11. Project settings preserve root and distinguish grouping

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Project settings → Name / Project icon / Pin / Checkouts / Danger

**Fixture.** Scratch project only

**Run.** Inspect checkouts and grouping scope; change scratch display name and restore; inspect remove/archive danger controls then cancel.

**Assert.** Name/icon affect intended logical project; checkout path/environment shown; remove checkout and delete project consequences explicit.

**Cleanup.** Restore name/icon/pin; cancel danger actions.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ProjectDetail / CheckoutGroupingSelect](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProjectSettingsPanel.tsx#L251). ProjectDetail edits the selected project or group. Checkout grouping is a separate choice. Verify root, current checkout, and scope labels.

### L12. Worktree controls disclose local changes and ownership

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `shared-client`.

**Entry.** Branch picker; Start worktree from remote; Settings → Worktrees

**Fixture.** Scratch clone only; no active user worktree

**Run.** Inspect base/branch choices and cleanup list. If parent creates a scratch worktree, verify thread/terminal cwd and dirty protection before cleanup.

**Assert.** New worktree has explicit path/branch; no source checkout change; Keep worktrees with local changes honored; cleanup never claims user worktree.

**Cleanup.** Close scratch thread/process first; remove only owned worktree when appropriate.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [WorktreesSettingsPanel / StartFromRemoteRow](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/WorktreesSettings.tsx#L54). Worktree defaults are scoped settings. Inspect cleanup and destination behavior only on an audit checkout with known ownership.
- [WorktreeCleanupSection / RetentionControl](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/StorageSettings.tsx#L548). Cleanup retention is scoped. Saving an age can start cleanup, so enabling first creates a draft. Inspect Keep worktrees with local changes without saving a cleanup rule.

## Check settings and remaining tools

### S01. Settings navigation/search and modal focus are complete

Status. **not-run**. Mode. `read-only`. Provider contract. `shared-client`.

**Entry.** Settings; Search settings; Settings sections; Settings content

**Fixture.** Existing config read-only; record starting view

**Run.** Open Settings, use search for Panel animations and a nonexistent term, navigate sections by keyboard, close with Escape/back.

**Assert.** Search reaches correct row and scope; no results is clear; focus stays in modal; close returns to prior thread without changing it.

**Cleanup.** Clear search; close modal.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [SettingsSidebarNav.handleSearchResultClick / handleSearchKeyDown](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsSidebarNav.tsx#L212). Settings search selects a section and target anchor. Verify focus and the visible control, not only the URL hash.

### S02. Scope is explicit and overrides do not silently affect all projects

Status. **not-run**. Mode. `read-only`. Provider contract. `environment-capabilities`.

**Entry.** Settings target/scope selector; Project defaults and overrides

**Fixture.** Scratch project and device scope

**Run.** Switch All/device, environment, project, checkout scopes where offered; inspect inherited value and unavailable-target notices; do not alter global provider settings.

**Assert.** Scope labels match actual target; inherited vs override distinguished; unsupported setting gives useful owning-scope navigation.

**Cleanup.** Restore original scope.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [useUpdateScopedSettings / useScopedSettings / useScopedSettingSource](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/useScopedSettings.ts#L100). A selected scope produces a write plan. Mixed values and inherited sources must be visible before any scratch-only preference change.
- [SettingsScopeBoundary](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/routes/_chat.settings.tsx#L45). Device-only and environment-backed pages have different scope requirements. An unavailable target must show its scope notice.

### S03. General controls are readable and reversible

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `shared-client`.

**Entry.** General → Language; Time format; Thread notifications; In-app notifications; Restore device defaults

**Fixture.** Record values; use read-only inspection first

**Run.** Inspect each row and selection menu at narrow/wide widths. Do not invoke Restore device defaults. Optionally change one harmless display value then restore.

**Assert.** Labels explain impact; keyboard selection works; disabled/read-only reasons visible; restore-default button is not mistaken for a local reset.

**Cleanup.** Restore any changed display value.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [GeneralSettingsPanel](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsPanels.tsx#L778). General reads scoped settings. Confirm current values and restore each temporary preference to its recorded value.

### S04. Appearance supports coherent theme, contrast, and motion

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Appearance → Color scheme / Themes / Contrast / Glass opacity / Panel animations / Interface font / Prompt font / Code font / Terminal font

**Fixture.** Recorded original appearance values

**Run.** Preview light/dark and a narrow panel; inspect typography controls. Parent may alter local appearance briefly and restore. Record both sidebar transitions at nonzero duration.

**Assert.** No unreadable text, clipping, low-contrast selection, wrong-color icons, or font-dependent control overlap; actual motion matches chosen setting.

**Cleanup.** Restore all appearance values.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [AppearanceSettingsPanel / isImportThemeOpen](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AppearanceSettingsPanel.tsx#L87). Appearance has scoped settings and a separate theme-import dialog. Test reversible presentation values without importing or replacing a theme.
- [usePanelAnimationSettings](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L31). Appearance motion is conditional on duration and reduced motion. Correlate the configured value with both opening and closing frames.

### S05. Chat preferences correspond to observed composer behavior

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `shared-client`.

**Entry.** Chat → Chat width / Word wrap / Composer context / Tool calls in chat / Render math / Rich text composer / Send shortcut / Follow-up behavior / Response streaming

**Fixture.** Scratch draft; avoid sending on changed shortcut

**Run.** Inspect settings and test harmless draft/selection behavior; if changing send shortcut, record and restore before any prompt.

**Assert.** Previewed control state matches actual chat; annotation/draft is preserved; Enter/newline/send meaning is clear; streaming display choice does not hide ongoing work.

**Cleanup.** Restore changed values and discard scratch draft.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ChatSettingsPanel / ToolCallVisibilityRow](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsPanels.tsx#L1140). Chat rendering and composer preferences read scoped settings. Reopen a scratch draft to verify the chosen behavior and restore it.

### S06. Diff preferences match both local and PR viewers

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `host-capabilities`.

**Entry.** Diffs & pull requests → Hide whitespace changes / Default diff file state / Diff layout / Open pull requests and changes panels proactively

**Fixture.** Scratch diff plus known PR

**Run.** Inspect defaults; if changed locally, reopen both viewers and compare.

**Assert.** Same preference has consistent meaning; proactive opening does not replace user-selected surface unexpectedly; changes are scoped honestly.

**Cleanup.** Restore values.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [DiffsSettingsPanel](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsPanels.tsx#L1501). Diff defaults are shared configuration. Compare their effect in local Diff and PR Code, including any per-view override.

### S07. Shortcut editor captures and cancels without trapping keys

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `shared-client`.

**Entry.** Keybindings → Search keybindings; Edit shortcut for <command>; When; Cancel new keybinding; Open keybindings.json

**Fixture.** No global shortcut changes needed

**Run.** Search panel/terminal commands; open editor, press harmless candidate chord, cancel; inspect conflict/when editor; do not save conflicts.

**Assert.** Capture receives chord without executing it; Escape cancels predictably; existing binding remains; search and condition text readable.

**Cleanup.** Cancel edits; do not alter keybindings.json.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [KeybindingsSettingsPanel / isAddingBinding / query](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/KeybindingsSettings.tsx#L1317). Shortcut creation has a separate draft state. Record conflicts, cancel the capture, and verify no binding was saved.

### S08. Providers expose availability without requiring credential access

Status. **not-run**. Mode. `inspect-only`. Provider contract. `agent-provider`.

**Entry.** Providers → Default model / Fallback model / Permissions / Usage providers / Text generators / Provider update checks

**Fixture.** Existing settings; parent controls actual Sol selection

**Run.** Inspect available-provider and health/status rows; account names only as necessary, never reveal/copy tokens. Do not sign in/out, update/install, change fallback, or start auth terminal.

**Assert.** Unavailable provider is explained; model/account/usage ownership clear; disabled actions give reasons; no fake success state.

**Cleanup.** Close settings; leave auth and provider preferences unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [EnvironmentProviderSettings / isRefreshingProviders / isAddInstanceDialogOpen](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSettingsPanel.tsx#L535). Provider cards are keyed by driver and instance. Capability, policy, connection, and permission checks gate edits. Inspect availability without reading auth material or changing providers.

### S09. Thread automation settings disclose consequences

Status. **not-run**. Mode. `inspect-only`. Provider contract. `shared-client`.

**Entry.** Threads → Auto-settle inactive threads / Compact idle threads / Auto-settle merged threads / Keep threads with a PR / Days of inactivity before auto-settle / Continue threads after restarts

**Fixture.** Read-only settings

**Run.** Inspect scope, dependencies and disabled rows; compare per-thread Auto-settle behavior → Follow settings / Never without changing user thread.

**Assert.** Rules and scope are understandable; unavailable server capability distinguished from off; no automation is accidentally enabled.

**Cleanup.** Leave all rules unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ThreadsSettingsPanel](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsPanels.tsx#L1708). The section reads environment, connected environments, target, scope, and mixed settings. Inspect auto-settle and continuation choices without enabling background work.

### S10. Browser defaults and access controls are understandable

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `host-capabilities`.

**Entry.** Browser → Agent browser access / Browser profiles / Default browser profile / Default browser viewport / Default browser zoom / Default browser appearance / Open links in / Search engine / Open pull requests in T3 Code

**Fixture.** Existing profile; controlled page only

**Run.** Inspect profile selector and defaults; test PR-link behavior against recorded preference; do not inspect cookies/passwords/profile files.

**Assert.** User navigation and agent access distinguished; disabled controls explain missing bridge; URL-opening preference matches actual result.

**Cleanup.** Restore any temporary link-opening preference; close audit browser tabs.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [BrowserSettingsPanel / PullRequestLinksOpenInAppSetting](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/IntegrationsSettings.tsx#L1581). Browser defaults, profile choices, link target, PR routing, and recording options are distinct. PR routing uses pullRequestLinksOpenInApp and must match the observed click.

### S11. Device settings have honest availability and safe cancellation

Status. **not-run**. Mode. `inspect-only`. Provider contract. `environment-capabilities`.

**Entry.** Devices → Device sources / Agents can use devices / Use devices / Local simulators

**Fixture.** No device creation or connection required

**Run.** Inspect available sources and empty/disconnected states; open add/source dialog only to cancel; do not launch simulators or attach private device data.

**Assert.** Prerequisites and authorization state explicit; unavailable device surface does not appear broken or blank.

**Cleanup.** Cancel dialogs; no device/session left running.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [DevicesSettingsPanel / DeviceIntegrationSettings](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/IntegrationsSettings.tsx#L1622). Device integration renders capability and connection state. Configuration can start or alter integration processes, so inspect and cancel.

### S12. Schedule editor validates without creating ongoing work

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `agent-provider`.

**Entry.** Scheduled tasks → New task; Schedule type; At a time / Once / Every interval; Enabled

**Fixture.** Scratch project selected in unsaved form

**Run.** Open New task; inspect project/workspace/destination thread, enter synthetic title/prompt, inspect schedule controls and invalid time/interval, then cancel.

**Assert.** Time zone and destination visible; invalid inputs block saving; description explains new vs existing thread/worktree; cancelling creates no task.

**Cleanup.** Cancel without Save/Run now; verify list has no audit task.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ScheduledTaskEditorForm.draft / submissionPending / activeSelection](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ScheduledTasksSettings.tsx#L1337). The editor draft includes schedule, project, checkout, destination, and model. Validation and cancel can be tested without saving or running the task.

### S13. Existing task management does not expose webhook secrets

Status. **not-run**. Mode. `inspect-only`. Provider contract. `shared-client`.

**Entry.** Actions for <task>; Deliveries; Edit task

**Fixture.** Existing tasks only if already relevant; otherwise skip

**Run.** Inspect action names and status metadata. Do not open/copy Webhook URL, expand request headers/body, run, enable, delete, or rotate token. Prefer an unsaved scratch editor.

**Assert.** Last-run status and errors readable; destructive/run controls distinguishable; task ownership/destination clear.

**Cleanup.** Close dialog; leave existing schedules unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ScheduledTaskRow.busy / deliveriesOpen](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ScheduledTasksSettings.tsx#L728). Task actions and webhook delivery details are separate from metadata. Do not run, enable, delete, rotate, reveal, or copy private webhook values.

### S14. Source-control settings match actual host behavior

Status. **not-run**. Mode. `inspect-only`. Provider contract. `host-capabilities`.

**Entry.** Source control → Automatically pull / Default merge method / Git fetch interval / Follow pull request templates / Commit and pull request writing / Source control writer model / Review guides (beta)

**Fixture.** Read-only settings

**Run.** Inspect defaults and host availability without triggering fetch/auth/generation. Inspect merge-method menu from PR without changing host.

**Assert.** Host, environment and writer-model roles are clear; auto-pull is not enabled inadvertently; templates and guide settings indicate scope.

**Cleanup.** Leave settings unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [SourceControlSettingsPanel](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SourceControlSettings.tsx#L395). Provider discovery and VCS status are distinct. Provider toggles change picker availability. Inspect account availability without exposing tokens or changing host configuration.

### S15. Worktree cleanup is transparent and safe to inspect

Status. **not-run**. Mode. `inspect-only`. Provider contract. `shared-client`.

**Entry.** Worktrees → Worktree cleanup / Keep worktrees with local changes / Worktree location / Delete now

**Fixture.** Read-only list; only audit-owned tree eligible for later removal

**Run.** Inspect listed paths, dirty protection, retention and cleanup preview if available. Do not press Delete now on user data.

**Assert.** List distinguishes active, dirty and removable entries; controls state exact scope; no silent cleanup during inspection.

**Cleanup.** Close panel; audit tree removal belongs after thread/process closure.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [WorktreesSettingsPanel](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/WorktreesSettings.tsx#L54). The section presents worktree configuration. A cleanup action must be matched to its actual confirmation and ownership checks before any later deletion.
- [WorktreeCleanupSection / RetentionControl](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/StorageSettings.tsx#L548). Cleanup retention is scoped. Saving an age can start cleanup, so enabling first creates a draft. Inspect Keep worktrees with local changes without saving a cleanup rule.

### S16. Environment connections report status without modifying topology

Status. **not-run**. Mode. `inspect-only`. Provider contract. `environment-capabilities`.

**Entry.** Environments → This machine / Local environment / Linked environments / Environments / Load balancing / Network access

**Fixture.** Current connections only

**Run.** Read status and capability summaries; inspect add connection form and cancel; do not disconnect/restart user server or change network exposure.

**Assert.** Connected/disconnected/unsupported states distinct; actions have clear target and error context; no credential details needed.

**Cleanup.** Cancel form; leave connections unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ConnectionsSettingsContent / addBackendDialogOpen / disableTailscaleServeDialogOpen](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ConnectionsSettings.tsx#L238). Environment setup, exposure, Tailscale, and WSL have separate pending and dialog state. Inspect status and cancel setup without altering connection topology.

### S17. Sharing controls distinguish local links from published access

Status. **not-run**. Mode. `inspect-and-cancel`. Provider contract. `environment-capabilities`.

**Entry.** Sharing & access → Guest links / Pull request sharing / Publish agent activity; Share with guest

**Fixture.** Scratch thread; existing grants read-only

**Run.** Inspect sharing dialog descriptions and permission scopes without creating a link or grant; cancel.

**Assert.** Who gains access and what they can do is explicit; scope includes intended thread/project; no automatic share on dialog open.

**Cleanup.** Cancel; verify no audit guest link/grant created.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [AuthorizedClientsHeaderAction / isCreatingPairingLink](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AuthorizedClientsSettings.tsx#L585). Creating a pairing link writes access state. Opening the dialog is safe, but creating, revealing, or revoking a real user link is outside this pass.

### S18. Secrets surface has safe empty and unavailable states

Status. **not-run**. Mode. `inspect-only`. Provider contract. `environment-capabilities`.

**Entry.** Secrets → Secret providers / Add secret provider

**Fixture.** Only if feature visible; no existing value inspection

**Run.** Open section; inspect provider type/options in unsaved add form, using no actual credential; cancel.

**Assert.** Unavailable/feature-gated state is clear; fields remain masked; validation cannot leak a value in toast.

**Cleanup.** Cancel; do not reveal, copy, rotate, or save secrets.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [SecretsSettingsPanel / EnvironmentSecretProviders](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SecretsSettings.tsx#L37). Secret providers are scoped to environments. Inspect the section and masked controls without revealing values or removing a provider.

### S19. Maintenance has useful diagnostics and explicit destructive actions

Status. **not-run**. Mode. `inspect-only`. Provider contract. `shared-client`.

**Entry.** Maintenance → Artifacts and logs / Diagnostics / OpenTelemetry export; diagnostics detail page

**Fixture.** Metadata only; no log export necessary

**Run.** Inspect headings, disk/process summaries and action labels; avoid raw logs that may include user data, stop/restart processes, cleanup, or telemetry export.

**Assert.** Diagnosis state is readable; actions identify process/retention scope and cancellation; no blanket success when backend unavailable.

**Cleanup.** Close diagnostic view; no process setting changed.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [MaintenanceSettingsPanel](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsPanels.tsx#L2132). Maintenance reads settings scope and connected environments. Identify read-only summaries separately from export, stop, reset, or cleanup actions.
- [CleanupSection.pending](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/StorageSettings.tsx#L373). Delete now invokes cleanup across its selected scope. Do not click during this read-only pass.

### S20. Labs and gated features explain their status

Status. **not-run**. Mode. `inspect-only`. Provider contract. `shared-client`.

**Entry.** Labs → Guest links / Linked environments / Suggest next prompt / Fast worktree checkout / Simlock device hosts (alpha) as available

**Fixture.** Installed build may omit or rename feature rows

**Run.** Inspect offered flags and descriptions; compare gating with related navigation, without enabling experimental features.

**Assert.** A gated/unavailable surface is labelled, not counted as a broken control; current source presence does not prove installed availability.

**Cleanup.** Leave flags unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [LabsSettingsPanel / GuestLinksRow / LinkedEnvironmentsRow](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/LabsSettingsPanel.tsx#L17). Labs exposes gated settings. Record visible availability and explanatory text without enabling capabilities or guest access.

### S21. About and update UI are complete without installing anything

Status. **not-run**. Mode. `read-only`. Provider contract. `shared-client`.

**Entry.** About → Version / Update track / t3 command / Mobile app / Welcome page / Privacy policy / Open source licenses

**Fixture.** Existing app build

**Run.** Record displayed version/build; inspect update track and release-note view if present; open licenses/privacy read-only. Do not install, restart, or change update track.

**Assert.** Installed version recorded against source pin; links open correctly; no invisible close/focus trap; update state is honest.

**Cleanup.** Close extra pages; return to original view.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [AboutSettingsPanel / AboutVersionSection](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsPanels.tsx#L2271). Version and update state are separate from installing or switching channels. Inspect the current version and links without applying an update.

### S22. Voice controls can be inspected without recording

Status. **not-run**. Mode. `inspect-only`. Provider contract. `agent-provider`.

**Entry.** Voice → Transcribe with / Microphone / Dictation shortcut / Transcription language / Dictionary / Cleanup model / Test microphone / Test model

**Fixture.** No audio recording in this checklist unless parent explicitly chooses it

**Run.** Inspect choices and capability text; open dictionary editor only to cancel. Test microphone/model may record/download/run a model: do not invoke during passive pass.

**Assert.** Input/output ownership and model prerequisite explicit; controls fit narrow settings panel; unavailable hardware has recovery.

**Cleanup.** Cancel editor; no recording/download/model process left active.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [VoiceSettingsPanel / SpeechModelBrowser / TranscriptionTest](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/VoiceSettingsPanel.tsx#L1191). Voice can start microphone capture, downloads, cleanup model calls, or test transcription. Inspect configuration and cancel without those side effects.

### S23. SnapShot options and saved-artifact privacy are clear

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `shared-client`.

**Entry.** SnapShots → Include app text / Capture shortcut / Capture sound / Capture flash / Capture animations

**Fixture.** Only synthetic scratch content on screen

**Run.** Inspect values; parent may capture one scratch-only image to test open/save. No user transcript or settings secrets in capture.

**Assert.** Capture has correct scope, readable result and completion feedback; animated capture includes real time sequence; save/open is reachable.

**Cleanup.** Close/delete only audit capture if not needed as evidence; restore preferences.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [SnapShotSettings.setupBusy / savedShortcut](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SnapShotSettings.tsx#L81). SnapShot setup and shortcut recording have separate state. Any actual capture must use a scratch-only visible target and be removed afterward.

### S24. Archive navigation is usable without modifying user history

Status. **not-run**. Mode. `inspect-or-local`. Provider contract. `shared-client`.

**Entry.** Archive; Unarchive <title>; Delete <title>

**Fixture.** Scratch archived thread if parent exercised L09; otherwise inspect only

**Run.** Search/scroll available archive list; inspect labels and target; restore scratch thread if present; cancel deletion.

**Assert.** Correct project/thread identity; empty/archive-unavailable state clear; restoring returns content; Delete not conflated with Unarchive.

**Cleanup.** Restore scratch thread, leave user archive unchanged.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ArchivedThreadsPanel.pendingKeys](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ArchivedThreadsPanel.tsx#L32). Archive list navigation is read-only. Unarchive and Delete are write actions scoped to a specific thread.

## Check cross-panel workflows

### X01. Browser opening does not disrupt PR/file work

Status. **not-run**. Mode. `local-reversible`. Provider contract. `host-capabilities`.

**Entry.** Browser tab; Add panel surface → Browser; address field

**Fixture.** Controlled local/static page; scratch Files and PR tabs

**Run.** Open URL, switch file/PR/browser rapidly, close browser and reopen; inspect nested chrome and loading/control positions.

**Assert.** Correct surface stays active; browser controls stable during load; panel does not become disproportionately wide; user inputs act on visible surface.

**Cleanup.** Close audit page tabs; stop parent local server when unused.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [resolvePullRequestTabLink / RightPanelTabs](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L735). PR tabs resolve their native target separately from Browser tabs. Opening a URL must not replace or duplicate an unrelated PR or file.
- [showExternalLinkContextMenu](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/externalLinkContextMenu.ts#L88). HTTP links offer Open in integrated browser when available, Open in system browser, and Copy Link. Each action calls a distinct handler.
- [useOpenLink / resolveLinkTarget](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/useOpenLink.ts#L21). The selected link preference, modifiers, target thread, and in-app availability decide the destination. App-open failure can fall back to the system browser.

### X02. Side chat can be opened and dismissed without losing parent draft

Status. **not-run**. Mode. `inspect-or-parent-agent`. Provider contract. `agent-provider`.

**Entry.** Add panel surface → Side chat

**Fixture.** Scratch parent thread; parent controls any Sol request

**Run.** Open empty side chat, type unsent marker, switch panel/thread, return, close with unsaved text. Actual send only by parent.

**Assert.** Parent and side-chat drafts separate; target/model context visible; close/reopen behavior honest; no background run starts from opening alone.

**Cleanup.** Discard audit draft; stop/close any parent-started side-chat agent.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [useSideChat.start / open](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useSideChat.ts#L26). Opening can fork a side-chat thread. A nonempty prompt also starts a turn. Track the child even when no model call occurs, then clean it up.
- [SideChatPane.parentRef / threadRef / mergeBack / deleteThread](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SideChatPane.tsx#L36). Side chat has its own scoped thread and explicit parent. Bring back and Delete are separate actions, not equivalents of closing the panel.

### X03. Subagent and background surfaces are distinct

Status. **not-run**. Mode. `parent-agent`. Provider contract. `agent-provider`.

**Entry.** Visible agent/subagent row; live-output or background-work control

**Fixture.** Parent creates one bounded Sol-run child/background fixture if supported

**Run.** Open child while streaming, select live output, navigate parent/back, close/reopen child tab, inspect nested agent if available.

**Assert.** The clicked child identity and transcript match. Provider-supported output updates appear before completion. The live-output action does not navigate to an unrelated empty Background view. Claude completed tool output is distinct from Codex streamed tool output. Reopening retains content and parent navigation returns to the correct thread.

**Cleanup.** Parent stops every created child and background process; verify closed.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [ChatView.providerSubagentStatus / isProviderSubagent / parentSubagentThreadRef](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatView.tsx#L833). Provider-native children have their own thread identity and status. Their read-only bar replaces the composer. Background tasks are derived separately from the projection.
- [ProviderSubagentBar.live](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14). Live status derives from isOrchestrationV2WorkActive. Parent must verify child output, parent navigation, status completion, and provider cleanup independently.
- [CodexProviderCapabilitiesV2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255). Codex declares streaming tool output, native subagent thread IDs, wait, close, and fork support. Record the actual installed provider and model before using this contract.
- [ClaudeProviderCapabilitiesV2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489). Claude declares assistant and reasoning streaming, no streaming tool output, and completed tool output. It emits subagent lifecycle but does not expose native child IDs or native wait, close, or fork capabilities.
- [claudeToolResultSettlesSubagent](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L3725). An asynchronous Agent launch acknowledgement does not settle a Claude child. The task notification supplies the completion event.
- [reportSubagentProgress](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L2889). A Codex native child running item updates its parent subagent progress row. Repeated identical progress is not emitted again.

Run and record these provider variants separately.

- Authorized Sol selection mapped to the observed Codex provider instance.
- Authorized Sonnet selection mapped to the observed Claude provider instance.

### X04. Notification/action overlays return to intended target

Status. **not-run**. Mode. `parent-agent`. Provider contract. `shared-client`.

**Entry.** Done / Approval / Input / Failed / Woke indicators; toast action

**Fixture.** Scratch run produces one bounded actionable state, parent-controlled

**Run.** Open notification/attention state from another selected thread; follow its action then back; dismiss toast with keyboard and pointer.

**Assert.** Correct thread/surface becomes visible on first action; toast does not obscure composer/panel controls; dismiss does not accidentally submit or select underlying UI.

**Cleanup.** Resolve/cancel audit prompt and stop run; clear only audit notifications.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [startThreadNotifications](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadNotificationCoordinator.tsx#L63). Notifications retain environmentId and threadId. Clicking navigates through buildThreadRouteParams. Validate the destination before testing any follow-up action.

### X05. Command palette routes to same destinations as visible controls

Status. **not-run**. Mode. `read-only`. Provider contract. `shared-client`.

**Entry.** Command palette; Open a surface / Add project / Project settings / known setting search

**Fixture.** Scratch thread; use shortcut displayed by installed app

**Run.** Open the same Files/Diff/PR/settings target through palette and visible controls; keyboard-select and Escape/back.

**Assert.** Same target and state; no hidden-thread routing; shortcuts usable with panel open; existing drafts and scroll position preserved.

**Cleanup.** Close palette and audit surfaces.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [OpenCommandPaletteDialog / searchState](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/CommandPalette.tsx#L404). Command search, browse navigation, and add-project flows have separate state. Compare resulting scoped destinations with their visible-button equivalents.

### X06. Final reopen/resume audit catches half-finished persistence

Status. **not-run**. Mode. `local-reversible`. Provider contract. `shared-client`.

**Entry.** Thread switching; close/reopen surface; app restart only if parent has authorized/planned one

**Fixture.** Completed scratch A/B flows; tracked owned processes

**Run.** Revisit every used surface from a different thread, then after closing its tab; where restart is in scope, capture state before/after.

**Assert.** No named but blank tab; no stale target or unexpected massive width; pending/failed operations explained; no orphan activity silently continues.

**Cleanup.** Complete global cleanup ledger below.

**Source match.** Read these bindings before the native action. Record the matching runtime state afterward.

- [migratePersistedRightPanelState / ThreadRightPanelState](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L485). Persisted panel state restores valid per-thread resources and excludes the standalone PR panel key. Distinguish intended persistence from leftover audit work.

## Gotchas

T3 has source support for both panel motion and a zero-duration default. The pinned default is zero. A lack of motion at that setting is expected. A nonzero setting that snaps, leaves invisible hit targets, or fails on close is a different result. H05 separates these states.

T3 explicitly activates the clicked thread after opening its PR in that thread's right panel. H02 tests this route change. A PR badge that underlines but leaves the old thread selected must not pass because an offscreen panel state changed.

PR Refresh invalidates host detail and activity. Code refresh has its own token and slice reset. Checks have a separate capability-gated query and refresh path. P12 requires observation of the displayed data and records the limit when a static PR provides no changed value.

Read-only PR fixtures cannot prove merge, publish, comment submission, approval, branch deletion, or access changes. An inspect-and-cancel result covers the control, target, and cancellation behavior only.

Some settings take effect as soon as they are saved. A cleanup retention change can start a sweep. Enabling a task, starting a guide, or supplying a side-chat prompt can create background work. The checklist does not treat these actions as passive inspection.

The Prove It Works principle changed the pass rule. Source presence cannot produce a live pass. The author verified all source anchors and the artifact structure, while leaving all native results unclaimed. The how skill's additional explorer and explainer workers were skipped under the parent's explicit no-more-agents instruction.

## Finish the cleanup ledger

Record every owned resource before creating it. Complete each row from direct observation.

| Resource | ID or path | Created by case | Stop or restore action | Verified state |
| --- | --- | --- | --- | --- |
| Scratch thread or draft | Record actual value. | Record case ID. | Stop active turn, then close or remove the owned audit item. | Record closed state. |
| Provider child or handoff thread | Record actual value. | Record case ID. | Parent stops or completes the run and closes the owned thread. | Record both run and thread state. |
| Terminal and process | Record terminal ID and PID. | Record case ID. | Stop audit process, then close terminal. | Record process absent and terminal closed. |
| Worktree or checkout | Record absolute path and branch. | Record case ID. | Close dependent threads and processes first. Remove only owned disposable work. | Record removed path or deliberately retained artifact. |
| Preference | Record scope and original value. | Record case ID. | Restore the recorded original value. | Reopen and read the value. |
| Draft review, quote, or attachment | Record target and audit marker. | Record case ID. | Remove only the audit material without submitting. | Reopen and confirm removed. |
| Browser or file tab | Record target and ownership. | Record case ID. | Close audit-created tabs. | Original tabs and selection preserved. |

The checklist author created no UI threads, model workers, terminal sessions, applications, or fixture processes for this task. The parent fills the cleanup ledger for live execution.

The machine-readable companion is [t3-live-tools-checklist.json](t3-live-tools-checklist.json). It retains the exact source anchors, result template, provider contracts, action boundaries, and initially empty evidence lists.
