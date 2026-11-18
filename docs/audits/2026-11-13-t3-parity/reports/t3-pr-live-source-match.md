# T3 PR live journeys matched to installed source

T3 completed the precise PR journey that failed in Ferrite: while another thread was selected, clicking Calculator UI Audit’s linked #246 badge activated Calculator and opened the correct native merged-PR detail. The installed implementation carries the clicked thread and the PR identity through both panel creation and navigation. This is the strongest direct baseline in this slice. It does not close the separately reproduced Ferrite failure.

The motion result is narrower: the parent captured an intermediate closing sidebar frame after setting Panel animations to 400 ms. Installed T3 defaults that setting to 0 ms. Opening and right-panel closing interpolation were not captured. Both facts belong in the comparison; neither dismisses the reported Ferrite snap.

## Evidence and version boundary

- Installed app: T3 Code Nightly `0.1.10-nightly.20261111.4544`; full commit `1264c206aee0dce805d6279809118bc091f29d2f`. See [installed-source provenance](t3-installed-source-match.md).
- The newer comparison checkout is `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. It is not the source authority for these native observations. The installed sidebar row, PR detail, Code tab, panel shell, and sidebar primitives differ from that checkout.
- Runtime owner: the parent agent. This worker matched recorded native results to source and performed no UI action. The [live observation ledger](../evidence/t3-live/observations.md) is the runtime record.
- Native screenshots cited by the ledger are inline conversation evidence. They were not exported by this worker, and this report invents no screenshot filename.
- Source collection: 45 official files read at the exact installed commit; two guessed paths returned 404. All 59 line anchors below were checked against those fetched texts. The JSON retains source and anchor hashes, not duplicated full source files.

## Coverage at a glance

| Case | Journey | Evidence boundary |
| --- | --- | --- |
| T3PR-LIVE-01 | Global PR entry, saved filters, and Escape return | Native completion observed |
| T3PR-LIVE-02 | Plain search works; repo qualifiers are deliberately ignored | Native partial; source explains limit |
| T3PR-LIVE-03 | Summary and Timeline load an explicit PR target | Native completion observed |
| T3PR-LIVE-04 | Code displays a real diff and discloses withheld content | Native completion observed |
| T3PR-LIVE-05 | Link to thread adds a local association | Native completion observed |
| T3PR-LIVE-06 | Clicking another thread PR activates that thread and native detail | Native completion observed |
| T3PR-LIVE-07 | Closing the sole PR tab and reopening the badge | Native completion observed |
| T3PR-LIVE-08 | Panel visibility keeps the active resource and releases hidden focus | Native completion in Browser context |
| T3PR-LIVE-09 | PR menus offer explicit destinations; right-click scope is specific | Sidebar right-click observed; list/detail actions unexecuted |
| T3PR-LIVE-10 | Refresh source reaches detail and activity; freshness is still untested live | Source only; refresh not exercised |
| T3PR-LIVE-11 | Default panel width is scoped and resize persistence is separate | Source only; geometry not measured |
| T3PR-LIVE-12 | Zero default and an actual animated sidebar close at 400ms | Native sidebar close only |

“Native completion observed” applies only to the described interaction. It does not certify adjacent controls, error paths, remote mutations, every provider, or a whole feature family. The Ferrite IDs below reference existing findings; these correspondence cases are not new defect counts.

## T3PR-LIVE-01: Global PR entry, saved filters, and Escape return

**Observed.** The parent opened Pull requests and saw actual authored rows with repository, state, checks, conflicts, age, and diff counts. Selecting Merged changed the results and route to state=merged. Escape returned to the prior thread. Reopening restored Merged. The thread sidebar remained available. Final cleanup cleared the PR query and restored state Open with involvement All.
Ledger section: **PR view entry**.

**Installed source.** The global utility action loads saved list preferences into /pull-requests. Filter changes update both storage and route search. The Escape observer returns through history only after other handlers decline the key. Selecting a PR creates the workspace detail target independently of the current thread panel.

[SidebarUtilityMenu.handlePullRequestsClick (SidebarChrome.tsx:234)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/sidebar/SidebarChrome.tsx#L234); [Route.validateSearch (_chat.pull-requests.tsx:31)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.pull-requests.tsx#L31); [PullRequestsPage.updateListScope (PullRequestsPage.tsx:258)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L258); [writePullRequestListPreferences (pullRequestListPreferences.ts:110)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/pullRequestListPreferences.ts#L110); [registerPullRequestsEscapeCommand (AppSidebarLayout.tsx:204)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AppSidebarLayout.tsx#L204); [PullRequestsPage.selectEntry (PullRequestsPage.tsx:1123)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1123); [PullRequestsPage.rightPanelRef / rightPanelAvailable (PullRequestsPage.tsx:180)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L180).

**Ferrite acceptance.** Existing findings: `pr-navigation-03`.

1. A PR entry must produce a visible, usable destination from the current context. A dedicated workspace, if implemented, retains its list scope independently of the selected detail.
2. Set Merged, leave the page, and return. The filter and displayed results agree. Escape closes a submenu before it leaves the workspace, and returning restores the previous thread and draft.

**Limits.**

- The right-panel control was disabled at the initial observed workspace state. This is not a claim that PR detail cannot open there. The source uses its own workspace panel ref and the parent subsequently opened detail.
- Only the recorded state filter was executed. Other filter controls were exposed, not exhaustively exercised. The Escape fallback at a fresh history entry was not tested.

## T3PR-LIVE-02: Plain search works; repo qualifiers are deliberately ignored

**Observed.** Plain ferrite produced Ferrite repository results. The repo:josephwylie/ferrite attempt left mixed results. The recorded native sequence did not establish exact submission timing.
Ledger section: **PR details and sidebar handoff**.

**Installed source.** Typed text and submitted text are separate state, with a 250ms debounce and explicit submit. The parser intentionally discards repo:, org:, user:, sort:, is:, archived:, and state: qualifiers because UI scope owns them. It does not convert repo: into a repository filter.

[usePullRequestQueryFilters (usePullRequestListView.ts:194)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/usePullRequestListView.ts#L194); [parsePullRequestQuery qualifier switch (pullRequestQuery.ts:96)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/client-runtime/src/pullRequestQuery.ts#L96); [Route.validateSearch (_chat.pull-requests.tsx:31)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.pull-requests.tsx#L31).

**Ferrite acceptance.** Existing findings: `pr-navigation-03`.

1. Document the supported search syntax next to the control. Either implement repository qualification or make its unsupported status visible rather than imply GitHub search semantics.
2. Apply repository/project scope through its visible control, then search plain text. Wait for the settled result and confirm every row belongs to the chosen scope.

**Limits.**

- Ignoring repo: is source-proven and consistent with mixed results. It is not proof of the exact runtime request, nor a newly certified T3 defect.
- No unexecuted author, label, draft, review, or checks filter is promoted into a live pass.

## T3PR-LIVE-03: Summary and Timeline load an explicit PR target

**Observed.** Opening merged Ferrite PR #246 showed its title, repository, branch pair, merged state, and a 194-file summary. Summary, Timeline, and Code were visible. Timeline showed real commits and review events.
Ledger section: **PR details and sidebar handoff**.

**Installed source.** Row selection carries repository, number, selected project, environment, and host into panel and URL state. usePullRequestDetail reads detail and activity under that target. Timeline derives its events from the merged current detail rather than the selected thread checkout.

[PullRequestsPage.selectEntry (PullRequestsPage.tsx:1123)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1123); [pullRequestPanelReference / pullRequestRefKey (usePullRequestDetail.ts:27)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L27); [usePullRequestDetail (usePullRequestDetail.ts:49)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L49); [PullRequestTimelineTab.events (PullRequestTimelineTab.tsx:758)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx#L758).

**Ferrite acceptance.** Existing findings: `pr-navigation-02`, `pr-navigation-04`.

1. Open a known PR through both the sidebar and Overview. The visible native header, repository, number, status, description, and Timeline identify the clicked PR.
2. A stored native PR target must remain explicit. Reading whichever PR happens to belong to the current branch cannot satisfy a click on another repository or PR number.

**Limits.**

- This establishes one merged PR and its visible events. It does not establish complete comment pagination, review replies, resolution controls, checks details, or mutation support.
- Ferrite native Summary and Timeline already worked through Overview in the earlier runtime correction. The repair target is the complete entry and target contract, not merely adding another PR component.

## T3PR-LIVE-04: Code displays a real diff and discloses withheld content

**Observed.** Code showed 100 files and Some of this diff was not shown. Expanding Cargo.toml revealed a +3 -2 diff. Split layout and the file tree changed visibly. The parent later restored Stacked and the global file-tree preference to hidden, confirmed by the Show file tree control. Hidden Summary text remained in the accessibility observation while the screenshot showed the correct Code pane.
Ledger section: **PR details and sidebar handoff**.

**Installed source.** Code tracks paginated diff slices and flags truncated or raw slices. The warning discloses withheld host content. Layout updates client preferences and the tree toggle changes its own persisted state. Previously visited detail tabs remain mounted and receive invisible styling when unselected.

[PullRequestCodeTab.withheldContent (PullRequestCodeTab.tsx:713)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L713); [PullRequestCodeTab truncation warning (PullRequestCodeTab.tsx:1250)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L1250); [PullRequestCodeTab diff layout setting (PullRequestCodeTab.tsx:1308)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L1308); [PullRequestCodeTab file tree toggle (PullRequestCodeTab.tsx:1435)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L1435); [PullRequestDetailPanel mounted Summary (PullRequestDetailPanel.tsx:1872)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L1872).

**Ferrite acceptance.** Existing findings: `pr-operations-007`.

1. Render the selected file patch and confirm line counts and content against the actual fixture. Split and Stacked must change the patch layout, and the tree must navigate the intended file.
2. Disclose partial data and the reason. A total from Summary must not be presented as fully reviewed when the visible diff is incomplete.
3. Verify the visible pane with a fresh capture. Test keyboard focus and assistive access separately if hidden tab content remains exposed.

**Limits.**

- The 194-file summary versus 100-file Code view does not prove a fixed 100-file cap or that every remaining file is inaccessible. Cursor state, host truncation, raw slices, and further pagination were not captured.
- The accessibility observation is not a certified screen-reader defect. CSS invisibility explains retained DOM, but actual assistive-tool behavior and hidden focusability were not exercised.

## T3PR-LIVE-05: Link to thread adds a local association

**Observed.** Link to thread selected the disposable Calculator UI Audit. That thread gained a merged #246 badge. No checkout or GitHub write was performed. In the later native cleanup, the parent selected Unlink from this thread and confirmed the sidebar PR badge disappeared, then closed the PR tab.
Ledger section: **PR details and sidebar handoff**.

**Installed source.** The picker selects a thread in the same environment. usePullRequestLinking parses the URL and sends T3 thread.pull-request.link. The PR repository identity remains in the link. Later target resolution can use an appropriate project to read that host without treating the thread checkout as the PR identity.

[ThreadPicker / EnabledPullRequestThreadLinks.changeLink (PullRequestThreadLinks.tsx:92)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestThreadLinks.tsx#L92); [usePullRequestLinking.changeLink (usePullRequestLinking.ts:52)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/usePullRequestLinking.ts#L52); [resolveChangeRequestLinkTarget (changeRequestLinks.ts:53)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/client-runtime/src/changeRequestLinks.ts#L53).

**Ferrite acceptance.** Existing findings: `pr-navigation-04`.

1. Associate the selected PR with the chosen audit thread and show its correct number and lifecycle. Verify the association after switching threads.
2. Keep thread ownership, checkout location, and remote PR identity separate. Linking must not silently checkout, publish, or mutate the remote PR.

**Limits.**

- Linking and later unlinking were exercised. Multiple linked PRs, cross-environment rejection, persistence across application restart, and linking failure were not tested.
- Local T3 association is an application write. The no-remote-write statement concerns the forge and repository actions, not the absence of all local state changes.

## T3PR-LIVE-06: Clicking another thread PR activates that thread and native detail

**Observed.** The parent activated the other disposable Sonnet thread, then clicked #246 on Calculator UI Audit. T3 selected Calculator UI Audit and opened its Workspace panel with the correct merged #246 native content.
Ledger section: **PR details and sidebar handoff**.

**Installed source.** The badge click passes the badge owner threadRef to the PR opener. The opener resolves and upserts the exact PR under that thread. Its successful native result returns true. handlePrClick then calls onThreadActivate for a non-route owner. Sidebar wires that callback to navigation using the same environment/thread pair.

[SidebarThreadRow.handlePrClick (SidebarThreadRow.tsx:955)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L955); [PullRequestBadge (ThreadStatusIndicators.tsx:241)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadStatusIndicators.tsx#L241); [useOpenPrLink (openPullRequestLink.ts:243)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/openPullRequestLink.ts#L243); [useOpenChangeRequestLink (openPullRequestLink.ts:50)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/openPullRequestLink.ts#L50); [resolveChangeRequestLinkTarget (changeRequestLinks.ts:53)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/client-runtime/src/changeRequestLinks.ts#L53); [pullRequestSurfaceId (rightPanelStore.ts:267)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L267); [upsertSurface (rightPanelStore.ts:309)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L309); [openPullRequest (rightPanelStore.ts:763)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L763); [SidebarThreadRow onThreadActivate (Sidebar.tsx:3417)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L3417); [Sidebar.navigateToThread (Sidebar.tsx:1447)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L1447); [SidebarThreadRow.handlePrListClick (SidebarThreadRow.tsx:1070)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1070).

**Ferrite acceptance.** Existing findings: `pr-navigation-01`, `pr-navigation-02`.

1. Start on A with its panel hidden. Click the rendered PR badge owned by B. On that click, B becomes selected and the intended native PR destination becomes visible.
2. Assert selected thread, project, PR number, active tab, and actual loaded body. A hidden B panel state change while A remains selected fails.
3. Repeat with A panel open, B already selected, and rapid A/B navigation. Keep explicit clicks separate from automatic agent reveals so the repair does not steal focus from background work.

**Limits.**

- The parent verified one plain cross-thread badge click and correct native destination. Modifier clicks, missing URL, multiple-PR list badges, empty board, and unavailable targets remain untested.
- Ferrite already reproduced the inverse path in pr-entry-runtime-correction.md. This T3 success supplies a real baseline; it does not rerun or close the Ferrite failure.

## T3PR-LIVE-07: Closing the sole PR tab and reopening the badge

**Observed.** The parent closed the sole PR panel tab. The panel hid. Clicking the sidebar badge reopened the correct #246 native surface.
Ledger section: **Panel motion and settings inspection**.

**Installed source.** closeSurface removes the requested tab and closes a panel with no remaining surfaces. Reopening through the badge upserts the explicit PR target, opens the panel, and selects that tab.

[closeSurface (rightPanelStore.ts:972)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L972); [pullRequestSurfaceId (rightPanelStore.ts:267)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L267); [upsertSurface (rightPanelStore.ts:309)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L309); [openPullRequest (rightPanelStore.ts:763)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L763); [SidebarThreadRow.handlePrClick (SidebarThreadRow.tsx:955)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L955).

**Ferrite acceptance.** Existing findings: `pr-navigation-02`.

1. Closing the last PR tab must close only that panel resource. The thread and unrelated draft remain available. Clicking its badge restores a usable native view of the same PR.
2. Reopening an already open PR activates its tab rather than duplicating it. Test this separately from last-tab closure.

**Limits.**

- The record confirms close and correct reopen, not PR scroll restoration, exact selected subtab restoration, data freshness, or application restart.
- Duplicate prevention is source-supported and an acceptance requirement; it was not independently exercised in this sequence.

## T3PR-LIVE-08: Panel visibility keeps the active resource and releases hidden focus

**Observed.** Command-Option-B hid and reopened the browser panel with its active tab retained. The unsent chat draft survived. Command-B hid and restored the left sidebar with composer focus.
Ledger section: **Browser, links and keyboard scope**.

**Installed source.** Visibility changes isOpen without deleting tabs or activeSurfaceId. The panel collapse effect returns focus before setting inert. Retained exit content is restricted to the same thread scope.

[close / toggleVisibility (rightPanelStore.ts:1102)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L1102); [PreviewPanelShell collapse layout effect (PreviewPanelShell.tsx:136)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L136); [usePanelPresence (panelAnimations.ts:41)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/panelAnimations.ts#L41).

**Ferrite acceptance.** Existing findings: `panel-tools-018`.

1. Hide and restore a panel containing an active PR or browser and an unrelated composer draft. Restore the same resource and preserve the draft.
2. Once hidden, panel controls and the browser must not consume later keyboard input or pointer hits. Verify actual focus rather than only checking the open Boolean.

**Limits.**

- The retained-resource live observation used Browser. The equivalent PR visibility-toggle branch is source-supported but not separately observed here.
- This does not establish process reclamation or memory behavior. Hiding a panel is not closing a browser session.

## T3PR-LIVE-09: PR menus offer explicit destinations; right-click scope is specific

**Observed.** More pull request actions showed link, refresh, question, explain, fixes, open-on-host, copy, and revert. The parent exercised Link to thread and later Unlink from this thread. A later right-click on the sidebar PR badge opened the thread context menu, including New thread, Pin, Settle, Snooze, Rename, and Copy Thread actions; no PR-specific external-open action was present in that observed menu. No forge mutation or checkout was performed.
Ledger section: **PR details and sidebar handoff**.

**Installed source.** The actions menu sends Open on host to shell.openExternal with detail.url. The workspace row number and detail breadcrumb have a custom right-click menu containing Copy link and Open on the provider host. The inspected sidebar badge has its click and anchor behavior but no dedicated PR context-menu handler in that component.

[PullRequestActionsMenu (PullRequestActionsMenu.tsx:243)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L243); [PullRequestActionsMenu open-on-host (PullRequestActionsMenu.tsx:398)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L398); [showPullRequestLinkContextMenu (pullRequestLinkContextMenu.ts:26)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts#L26); [PullRequestRow number context menu (PullRequestRow.tsx:149)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestRow.tsx#L149); [openNumberContextMenu (PullRequestDetailCrumbs.tsx:19)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx#L19); [PullRequestBadge (ThreadStatusIndicators.tsx:241)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadStatusIndicators.tsx#L241).

**Ferrite acceptance.** Existing findings: `pr-navigation-02`.

1. Native PR entry and external open must be distinct, reliable actions. External open uses the URL of the PR being displayed, not a stale branch fact.
2. Right-click the workspace row number and detail number. Verify the offered external action opens the exact target. Audit the sidebar badge separately; do not infer identical menus across locations.
3. Keep Copy link, external open, and local thread association distinct. A link that underlines must perform its advertised navigation.

**Limits.**

- Sidebar PR right-click was observed and produced the thread menu. The PR-specific list/detail context menus and external opening were not executed; those remain source-supported acceptance checks.
- The observed sidebar context menu must not be generalized to list/detail numbers or webpage links. Different locations have different handlers.

## T3PR-LIVE-10: Refresh source reaches detail and activity; freshness is still untested live

**Observed.** Refresh was present in the PR actions menu. The current T3 ledger does not record a click against a changed response or observed freshness after refresh.
Ledger section: **PR details and sidebar handoff**.

**Installed source.** The menu invokes refreshFromHost. Its refetch invalidates host cache, refreshes detail, activity, and stack, and increments a token that resets Code slices and refreshes viewed files. Checks use their own capability-gated query and refresh path.

[PullRequestActionsMenu (PullRequestActionsMenu.tsx:243)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L243); [refetchFromHost / refreshDetail (PullRequestDetailPanel.tsx:665)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L665); [PullRequestCodeTab appliedRefreshToken (PullRequestCodeTab.tsx:540)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L540); [usePullRequestDetail (usePullRequestDetail.ts:49)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L49).

**Ferrite acceptance.** Existing findings: `pr-operations-001`.

1. With one PR open, change checks, body, and comments in a controlled read fixture while keeping its number and lifecycle unchanged. Trigger the actual visible refresh action.
2. The native detail must update all displayed data consistently. Verify real reads and resulting text. A spinner or refreshed sidebar badge cannot pass.
3. Keep old useful content visibly stale during a pending read and expose a recoverable error if refresh fails.

**Limits.**

- This report does not claim T3 refresh freshness was live verified. It records the exact source path and the missing live condition.
- Ferrite stale refresh is already runtime-reproduced in both CI directions. Reopening or lifecycle changes worked there, so those recovery paths do not satisfy the failing same-identity refresh contract.

## T3PR-LIVE-11: Default panel width is scoped and resize persistence is separate

**Observed.** The parent observed opening, closing, and reopening panels. No controlled T3 logical-pixel width sequence or manual-divider experiment is recorded in this slice.
Ledger section: **Panel motion and settings inspection**.

**Installed source.** The chat right panel uses a per-thread width key and a 540px inline default. The standalone PR workspace instead uses t3code:pull-request-panel-width and a half-window default (640px server fallback). Both reach the shared 360px minimum, 70%-of-viewport cap, and container-minus-360px sibling cap, with the maximum kept at least the minimum. A drag commits saved width at its end. The left sidebar has a separate 256px default and its own bounds. These are CSS-pixel source values, not measurements of the parent screenshots.

[usePreviewPanelInlineSize (usePreviewPanelInlineSize.ts:14)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/usePreviewPanelInlineSize.ts#L14); [ChatRightPanel widthStorageKey (ChatView.tsx:3935)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3935); [useResizableWidth.commitWidth / drag.finish (useResizableWidth.ts:63)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/useResizableWidth.ts#L63); [resolveInitialThreadSidebarWidth (threadSidebarWidth.ts:25)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/threadSidebarWidth.ts#L25); [PreviewPanelShell.widthTransition (PreviewPanelShell.tsx:82)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L82); [PullRequestsPage standalone PR width props (PullRequestsPage.tsx:1508)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1508); [RightPanelTabs PreviewPanelShell props (RightPanelTabs.tsx:1335)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelTabs.tsx#L1335).

**Ferrite acceptance.** Existing findings: `panel-tools-018`.

1. Record the starting compact panel width. Visit Files or Changes and return to the same compact content without touching the divider. Recover the original compact width.
2. Repeat after closing the tool tab, hiding/reopening, emptying tabs, and restarting. A deliberate operator resize persists independently from a temporary tool minimum.
3. At narrow widths preserve a usable conversation or use a deliberate alternate layout. Do not silently turn an automatic expansion into the saved operator preference.

**Limits.**

- Do not convert the screenshot sidebar width into CSS pixels without a verified scale. This report makes no native T3 geometry measurement.
- The 540px chat default and half-window standalone PR default are not universal design requirements for Ferrite. The relevant requirement is predictable sizing and separation of automatic limits from saved user intent.
- Ferrite earlier reproduced 320px Overview becoming a persistent 864px panel after Files or Changes. That remains separate runtime evidence, not a measurement from T3.

## T3PR-LIVE-12: Zero default and an actual animated sidebar close at 400ms

**Observed.** Appearance Advanced initially showed Panel animations at 0ms. The parent temporarily selected 400ms and captured a partially translated closing left sidebar before fully closed frames. Decrement stepped through 25ms intervals to zero. A later settings search showed zero. Opening and right-panel closing captures were already at their endpoints.
Ledger section: **Panel motion and settings inspection**.

**Installed source.** The installed contract allows 0 to 400ms and defaults to zero. The slider writes 25ms steps. Positive duration activates motion only without reduced-motion or route suppression. Desktop sidebar gap width and fixed-body position transition together. The right panel separately retains same-thread exit content and animates visibility changes while suppressing unrelated width transitions.

[PanelAnimationDurationMs defaults and bounds (settings.ts:155)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/contracts/src/settings.ts#L155); [AppearanceSettingsPanel duration slider (AppearanceSettingsPanel.tsx:374)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/settings/AppearanceSettingsPanel.tsx#L374); [usePanelAnimationSettings (panelAnimations.ts:31)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/panelAnimations.ts#L31); [usePanelNavigationSuppression (panelAnimations.ts:11)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/panelAnimations.ts#L11); [AppSidebarLayout.sidebarProviderStyle (AppSidebarLayout.tsx:451)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AppSidebarLayout.tsx#L451); [Sidebar offcanvas transition (sidebar.tsx:369)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/sidebar.tsx#L369); [usePanelPresence (panelAnimations.ts:41)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/panelAnimations.ts#L41); [PreviewPanelShell.widthTransition (PreviewPanelShell.tsx:82)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L82).

**Ferrite acceptance.** Existing findings: `motion-001`, `motion-002`.

1. At a chosen nonzero duration, record intermediate frames for both sidebar opening and closing. Closing content remains visible during the exit and becomes inert before hidden controls can receive input.
2. Test right-panel opening and closing separately. Rapid reversal must not leave blank content, invisible hit targets, or stale content from another thread.
3. Record the reduced-motion setting and verify the resulting behavior with an authorized controlled preference change. Restore the original app duration and confirm its displayed value.

**Limits.**

- Only the left-sidebar closing transition at 400ms is directly established. Opening interpolation, right-panel interpolation, timing accuracy, frame rate, and subjective smoothness remain unverified.
- Reduced-motion suppression is source-proven here, not a live preference-toggle test. The settings preview is a separate widget and cannot substitute for actual sidebar motion.
- The installed zero default does not negate the user report that Ferrite snaps. T3 proves a configurable animated close path; Ferrite acceptance should follow the desired product behavior rather than copy zero motion by default.

## Preference boundaries

Ordinary browser links were set to external and proactive PR/Changes panels were off. Diff Split was restored to Stacked. The detailed ledger independently records Stacked restoration.

Ordinary browser link destination, in-app PR routing, and automatic panel reveals are separate settings. A proactive-off setting must not disable an explicit badge click. The native cross-thread result proves the exercised PR route succeeded, not every preference combination.

[PullRequestLinksOpenInAppSetting (IntegrationsSettings.tsx:782)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/settings/IntegrationsSettings.tsx#L782); [DiffsSettingsPanel proactive preference (SettingsPanels.tsx:1517)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/settings/SettingsPanels.tsx#L1517); [useOpenChangeRequestLink (openPullRequestLink.ts:50)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/openPullRequestLink.ts#L50).

## Provider boundary

These PR paths use shared UI plus environment and forge capabilities. Moving from the other Sonnet thread into Calculator demonstrates target ownership across those thread contexts; it does not establish a model-specific PR capability or certify every provider/host combination. Detail checks have their own capability gate. Source: [useOpenChangeRequestLink](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/openPullRequestLink.ts#L50); [resolveChangeRequestLinkTarget](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/client-runtime/src/changeRequestLinks.ts#L53); [usePullRequestDetail](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L49).

## Existing Ferrite evidence used

- [PR runtime correction](pr-entry-runtime-correction.md): cross-thread badge leaves the wrong thread selected and its destination invisible; focused-thread badge opens Browser; Overview reaches working native Summary and Timeline. Same-identity refresh leaves native checks, body, and comments stale.
- [Panel runtime correction](panel-runtime-correction.md): compact Overview grows from 320 px to 864 px after Files or Changes without divider input, then keeps that size across the recorded return, closure, and restart paths.
- [PR navigation](pr-navigation.md), [PR operations](pr-operations.md), and [motion](motion.md): original finding IDs and their earlier evidence boundaries. Source-only motion findings are not silently upgraded by T3’s separate runtime result.

## Scope, cleanup, and validation

This correspondence used the pstack “Prove It Works” rule: completion, interpolation, data freshness, and actual destination require observed behavior. Source presence and visible menu entries do not substitute for those results.

- No live UI action by this worker. Parent observations are attributed rather than rerun.
- No app code, live ledger, checklist, installed app, source checkout, credentials, or fixture was changed.
- No model worker, app, server, test, typecheck, build, or delegate was started. Only finite read-only source collection commands ran and exited.
- No new consolidated Ferrite finding IDs or severity promotions. T3 evidence does not close Ferrite failures.
- Two guessed source paths returned 404. Those path guesses are retained as collection metadata, not interpreted as missing capabilities.
- Inline native screenshots are conversation evidence. They were not exported by this worker and no PNG path is fabricated.

Validation checked 45 source-content hashes, 59 exact installed-commit line anchors, every case-to-anchor reference, 12 unique case IDs, and the current ledger fingerprint. No tests, typecheck, build, fixture, app, model, or delegate was started. This worker has no active task-owned runtime process to stop. Parent-owned UI cleanup remains with the parent.

The companion [machine-readable correspondence](t3-pr-live-source-match.json) preserves all statuses, assertions, limitations, source URLs, and hashes.

## Bounded cleanup follow-up

The parent’s later native observation resolves the sidebar right-click branch: right-clicking the PR badge opened the thread context menu (New thread, Pin, Settle, Snooze, Rename, Copy Thread and related actions), with no PR-specific external-open action. The list/detail PR menus remain separate source-supported paths. The parent then used **Unlink from this thread**, observed the sidebar PR badge disappear, and closed the PR tab. This extends the local-association case to unlinking without certifying any forge mutation.

**Restoration is verified in the final cleanup ledger.** The global file-tree preference is hidden and the control showed **Show file tree**. Global diff layout is **Stacked**. The PR query is empty, state is **Open**, and involvement is **All**. PR #246 is unlinked, its badge is absent, and the opened PR tabs are closed. See [Final child-output recheck and cleanup](../evidence/t3-live/observations.md#final-child-output-recheck-and-cleanup).

The source reason this required explicit restoration remains relevant: PR Code uses the unscoped localStorage key `t3code.pullRequestFileTreeOpen`, default `false`. The toggle writes that shared persistent Boolean. Closing a PR tab or archiving the audit thread does not clear it. Source: [PULL_REQUEST_FILE_TREE_STORAGE_KEY](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L151); [PullRequestCodeTab.fileTreeOpen](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L398); [useLocalStorage.setValue](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/useLocalStorage.ts#L206); [PullRequestCodeTab file tree toggle](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L1435); [usePullRequestLinking.changeLink](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/usePullRequestLinking.ts#L52).

Panel animations was also verified at its original 0 ms. The ledger records both disposable roots archived through normal UI. This worker performed no UI or application-storage mutation. These cleanup observations and the one recorded cross-thread PR click do not certify every PR state, keyboard modifier, context action, or remote operation. No remote PR write is claimed.
