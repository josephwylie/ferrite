# PR navigation investigation

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants Ferrite's PR workflow to be reachable and reliable, including sidebar PR links that visibly open the intended PR, clear project scope, and predictable navigation. This report compares the complete PR navigation chain with current T3 Code and separates source defects from the installed-app behavior that still needs reproduction.

## Overview

Ferrite has a native PR view. It is reachable through the Overview's Changes PR row, the panel's + menu, the empty-panel opener, and the command palette's "Open a surface" menu. The legacy Review slot also has its own PR display. Calling the whole PR feature absent would be incorrect.

The primary navigation problem is that the sidebar PR number and footer PR button use a different route. They call `open_pr`, which opens the owning thread's Browser. Clicking another thread's number stops the card's activation event, and the browser reveal never focuses that thread. The visible panel remains on the focused thread. This is a source-proven cause of an underlined link appearing to do nothing. It does not establish that every reported dead link has this cause.

There are five substantive findings and one verification gap. One finding is P1 and five are P2. No runtime reproduction, tests, builds, or performance measurements were performed by this read-only worker. No application source, state, dependencies, branches, or user data were changed.

## Method and source identity

- Ferrite baseline and inspected HEAD: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- T3 baseline and inspected HEAD: `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, local checkout `/tmp/ferrite-t3-audit-source-20261113`, origin `https://github.com/pingdotgg/t3code.git`.
- Used `rg` to find all `open_pr`, `open_pr_surface`, PR menu actions, `SurfaceId::PullRequest`, browser reveals, and T3 PR routes and link handlers. Read event handlers, state structures, loaders, rendering selectors, persistence, and relevant tests. Searches also covered Factory GitHub starts, task delivery links, generic page links, and the legacy Review slot before calling the PR inbox absent.
- Ferrite citations are repository-relative paths and line ranges at the baseline. T3 citations below are pinned to the exact commit. ADR 0039 provides intent only; implementation findings come from code. Historical T3 specs were not treated as current evidence.
- Read the common brief, Codex harness adapter, poteto investigation guidance, and unslop writing guidance. Followed the read-only investigation path and used source evidence rather than adding a prototype. The parent owns live UI access.

## Key concepts

Ferrite's PR has three different contexts. The thread's `facts.pr` is a lightweight branch reading. Its URL drives sidebar browser navigation. The native panel stores `PullRequest(number)` beneath a thread and loads the current checkout's PR into `pr_reads[thread]`. Factory task delivery separately stores a task PR and opens its URL through the OS. These are not one shared selected-PR target. See `crates/ferrite-core/src/workspace/mod.rs:539-557`, `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:274-305`, and `crates/ferrite-cockpit/src/cockpit/delivery.rs:108-122`.

A panel belongs to the focused pane. Having a tab in some other thread's panel does not make that tab visible. `panel_candidate_index` uses `focused()` and returns none on the empty board or Factory view. See `crates/ferrite-cockpit/src/cockpit/panel.rs:181-211`. This distinction is central to the dead-link finding.

T3 uses an explicit PR target containing project, repository, number, optional host, and optional environment. Its selected PR is independent of its list filter scope. Its right-panel identity includes the target scope, while the route separately records `selectedProjectId` and `selectedEnvironmentId`. See [target identity](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L267-L298), [scope resolution](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestListView.ts#L89-L123), and [selected entry](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1123-L1132).

## Traced entry points

| Affordance | Actual route and visibility | Evidence |
| --- | --- | --- |
| Sidebar PR number, plain press | Mouse-down calls `open_pr(thread)`; click propagation is stopped; browser opens for that thread without activating it. | `crates/ferrite-cockpit/src/cockpit.rs:12806-12819`, `:1826-1831`; `crates/ferrite-cockpit/src/cockpit/browser.rs:637-647` |
| Sidebar PR number, platform modifier | Uses OS URL opener when URL exists. It does not use native PR detail. | `crates/ferrite-cockpit/src/cockpit.rs:12813-12815` |
| PR number hover popover | The popover row opens the URL externally if it exists. Hovering and clicking can therefore use a different destination than pressing the number itself. | `crates/ferrite-cockpit/src/nav/card.rs:820-858` |
| Sidebar footer PR icon | Selects the focused thread with a known PR, then calls the Browser route. With no PR it is dimmed and click does nothing. Its comment claims the thread's PR in the panel, but it does not select the native PR panel. | `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:314-339` |
| Thread context menu, "Open PR in browser" | Enabled only when a URL exists, then calls `open_pr` for that context thread. The named browser behavior is intentional; a nonfocused context target has the same visibility issue as the badge. | `crates/ferrite-cockpit/src/cockpit/context_menu.rs:460-462`, `:854` |
| Palette and `OpenPrInBrowser` action | Uses focused thread, gated by URL in palette. Browser destination matches its label. | `crates/ferrite-cockpit/src/cockpit/palette.rs:1090-1096`, `:1307-1310`; `crates/ferrite-cockpit/src/cockpit.rs:10488-10491` |
| Overview Changes PR row | `open_pr_surface(thread)` creates or activates native PR tab, opens panel, loads snapshot. Row appears with a known PR even if there are no edited files. | `crates/ferrite-cockpit/src/cockpit/panel/changes.rs:19-45`, `:379-397`; `crates/ferrite-cockpit/src/cockpit/panel.rs:254-266` |
| Panel + menu and empty panel | "Pull request" is unavailable when `facts.pr` is absent. Otherwise it calls `open_pr_surface`. | `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:136-187`, `:637-638` |
| Palette "Open a surface" | Opens the focused thread's panel if hidden, then its opener menu. This is an additional route to native PR detail. | `crates/ferrite-cockpit/src/cockpit/palette.rs:914-921`, `:1286`; `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:622-634` |
| Existing native PR tab | Activation selects the persisted tab; render selects Summary, Guide, Timeline, or Code. | `crates/ferrite-cockpit/src/cockpit/panel.rs:281-284`, `:389`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:452-469` |
| Native PR header number, "Open on GitHub" | Calls `open_pr(thread)` and thus switches to the embedded Browser, using facts URL rather than the snapshot URL displayed in its breadcrumb. | `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:492-532`; `crates/ferrite-cockpit/src/cockpit.rs:1827-1831` |
| Legacy Review slot PR token and `p` | Focuses Review, emits `PrAsked`, reads current thread checkout, fills the Review slot's PR display. This is separate from the newer native panel. | `crates/ferrite-cockpit/src/review.rs:796-805`, `:2378-2384`, `:2691`; `crates/ferrite-cockpit/src/cockpit/beside.rs:255-257`, `:410-427` |
| Factory task delivery PR | Direct OS URL opener; does not select thread native PR detail. | `crates/ferrite-cockpit/src/cockpit/delivery.rs:108-122` |
| Generic page/website URL | HTTP and HTTPS become browser requests; no PR target or PR anchor is parsed at this route. | `crates/ferrite-pane/src/preview.rs:301-331`; `crates/ferrite-cockpit/src/cockpit/browser.rs:342-355` |
| PR checks log row | Opens a run URL in thread Browser. This is a log destination, not a missing native PR route. | `crates/ferrite-cockpit/src/cockpit.rs:12091-12109` |
| Factory "From a GitHub pull request" | Configures task creation from matching PRs. Its match count and backlog actions do not form a browse/select PR review page. | `crates/ferrite-cockpit/src/cockpit/starts.rs:790-825`, `:1055-1074` |

## State, scope, and lifecycle

Native opening deduplicates by `SurfaceId::PullRequest(number)` inside one thread. `upsert` selects the tab and sets `open=true`. The chosen PR subview is stored per number. Closing and reopening are supported. These behaviors are explicit in `crates/ferrite-cockpit/src/cockpit/surfaces.rs:114-134`, `:165-185`, and `:231-235`.

`panel.json` retains each thread's tabs, active tab, width, maximization, and PR subviews. PR tabs survive restart and load when shown. See `crates/ferrite-cockpit/src/cockpit/surfaces.rs:340-363`, `:370-400`, and `crates/ferrite-cockpit/src/cockpit/panel.rs:124-139`, `:368-390`.

The PR body does not load by stored number. `read_pr_snapshot` obtains the thread's effective checkout and requests its current branch PR. Once another PR number is found, `retire_other_prs` removes tabs for the previous number. That protects against a tab acting on a differently named PR but also proves that independent historical PR tabs are unsupported. This is a capability boundary, not evidence that repositories with the same PR number currently collide. Thread nesting already separates different threads. See `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:283-305`, `:319-352` and `crates/ferrite-cockpit/src/cockpit.rs:3508-3521`.

Browser reveal is delayed until the pump. The pump selects Browser in the target thread only if its user-action revision still matches. It never changes the focused thread. If that thread has no pane, the reveal remains queued. Attachment also requires a watched thread or existing ownership, so an unwatched target can fail before any URL opens. The reveal entry is created before attachment and is not removed by that failure. A later thread activation may therefore reveal the pending Browser state or its error, rather than a successfully opened URL. This explains the source mechanism for apparent delayed navigation; whether the installed app visibly exhibits the delayed variant requires a runtime check. See `crates/ferrite-cockpit/src/cockpit/browser.rs:226-228`, `:637-647`, `crates/ferrite-cockpit/src/cockpit/surfaces.rs:155-162`, and `crates/ferrite-cockpit/src/cockpit/panel.rs:204-211`. The backend gate is `crates/ferrite-core/src/server/browser.rs:116-130`, called at `:146-165`; failed attachment is stored at `crates/ferrite-cockpit/src/cockpit/browser.rs:237-240`.

## Detailed findings

### pr-navigation-01: Clicking another thread's PR routes to a panel that is not shown

Priority P1. Category `defect`. Evidence `source-proven`.

A plain click on the underlined PR badge of a nonfocused thread does not reveal its destination. The browser request targets that thread, while the visible panel remains attached to the focused thread. If browser ownership is unavailable, attachment fails and its error is also stored under the hidden target. This directly explains a class of apparent dead sidebar links.

Trigger: Focus thread A, then click thread B's PR badge. B may already have a pane, may be absent from the board, or may appear while the empty board is shown.

Expected and actual: An explicit click should visibly open B's PR. Instead, the card handler that normally invokes `open_nav_row` never runs because the badge stops propagation. Browser state changes under B, while the renderer still uses A or no panel. If B is not watched and has no existing browser ownership, the server rejects attachment; the UI stores that error under B instead of revealing it.

Cause: The opener mutates per-thread browser state without performing the visible navigation that its caller needs. T3 explicitly activates the target thread after a successful right-panel open.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:12806-12819`
- `crates/ferrite-cockpit/src/cockpit.rs:1826-1831`
- `crates/ferrite-cockpit/src/cockpit/browser.rs:218-240`
- `crates/ferrite-cockpit/src/cockpit/browser.rs:637-647`
- `crates/ferrite-cockpit/src/cockpit/panel.rs:204-211`
- `crates/ferrite-cockpit/src/cockpit/nav_list.rs:327-337`
- `crates/ferrite-core/src/server/browser.rs:116-130`
- `crates/ferrite-core/src/server/browser.rs:146-165`

T3 evidence:

- [apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969)

Recommendation: Route an explicit PR click through a destination-aware navigation operation that makes the clicked thread or selected PR visible. Keep agent-initiated browser reveals separate from explicit user navigation.

Observable acceptance check: With A focused and B visible in the sidebar, click B's PR number. B's exact PR becomes visible on that click. Repeat with B without a board pane, with the panel hidden, and from the empty board. A modifier click opens the external URL without switching the thread.

Limit: No installed-app click was performed. The route and focus omission are deterministic in source; hit-testing and native browser startup remain unverified.

### pr-navigation-02: The main PR affordances bypass the native PR view

Priority P2. Category `interaction`. Evidence `source-proven`.

The sidebar PR badge and footer PR button open a generic Browser tab, while the Overview PR row and panel menu open the native Summary/Guide/Timeline/Code view. Users can repeatedly choose the obvious PR controls without discovering the built-in review interface.

Trigger: Click the PR badge or footer while its thread is focused and has a valid URL.

Expected and actual: A clearly named primary PR affordance should make the existing PR review UI discoverable. Actual routes split between generic Browser, native PR, and external browser according to the exact affordance.

Cause: Recent native panel work added `open_pr_surface` but the principal nav call sites still use the older browser opener. ADR 0039 establishes one tab host, not proof that the old browser calls are accidental. This is an interaction gap against the requested workflow, rather than a claim that named browser commands are defective.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:314-339`
- `crates/ferrite-cockpit/src/cockpit.rs:1826-1831`
- `crates/ferrite-cockpit/src/cockpit.rs:12806-12819`
- `crates/ferrite-cockpit/src/cockpit/panel/changes.rs:379-397`
- `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:159-171`
- `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:452-469`

T3 evidence:

- [apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969)
- [apps/web/src/lib/openPullRequestLink.ts#L74-L137](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/openPullRequestLink.ts#L74-L137)

Recommendation: Make the badge and current-PR action open the native PR view by default. Preserve separately named browser and external-browser actions. Do not relabel the existing Open PR in browser command as broken merely because it does what its name says.

Observable acceptance check: For the focused thread, the badge and current-PR action show the same native PR tab as Overview and the + menu. Repeated opens leave one native tab. Explicit browser actions still open the browser.

Limit: The native panel can still be reached through Overview and the surface menu. Runtime validation should avoid saying it is wholly inaccessible.

### pr-navigation-03: There is no standalone PR inbox or project-scoped PR browsing destination

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Ferrite cannot use the sidebar footer to browse PRs independently of a focused branch. With no known PR on the focused thread, that control does nothing even when the project has other PRs. Reviewers cannot inspect authored, reviewing, or other project PRs through an app-wide list.

Trigger: Open the footer PR control with no focused thread, no known branch PR, or a project whose other PRs need review.

Expected and actual: T3 provides a persistent standalone list and selected detail. Ferrite provides a branch shortcut and separate Factory PR task inputs, with no equivalent global browse/select destination in the inspected code.

Cause: Ferrite's panel and PR entry models depend on thread context and current branch discovery. Its nav project filter filters navigation rows, not a separate PR list.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:324-339`
- `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:136-144`
- `crates/ferrite-cockpit/src/cockpit/surfaces.rs:25-45`
- `crates/ferrite-cockpit/src/cockpit/starts.rs:790-825`
- `crates/ferrite-cockpit/src/cockpit/starts.rs:1055-1074`

T3 evidence:

- [apps/web/src/components/sidebar/SidebarChrome.tsx#L234-L239](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarChrome.tsx#L234-L239)
- [apps/web/src/routes/_chat.pull-requests.tsx#L30-L78](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/routes/_chat.pull-requests.tsx#L30-L78)
- [apps/web/src/components/pullRequest/usePullRequestListView.ts#L89-L123](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestListView.ts#L89-L123)
- [apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1123-L1132](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1123-L1132)

Recommendation: Add a distinct PR inbox destination with project scope and explicit selection, usable without a focused thread. Keep the current branch shortcut separate. Scope and filter controls should represent actual supported backend queries.

Observable acceptance check: From an empty board and a thread without a PR, open Pull requests, select a project, inspect a PR from a different branch, change list scope, and return to the previously selected PR without changing checkout.

Limit: This is missing current T3 capability, not an assertion that every T3 list filter is mandatory for Ferrite. Implementing the inbox requires a product decision about scope and backend support.

### pr-navigation-04: The native PR target is the current branch, so links and multiple PRs cannot become independent native tabs

Priority P2. Category `partial-capability`. Evidence `source-proven`.

A native tab stores only a number under a thread, and its data loader resolves the current checkout rather than an explicit repository and PR. Ferrite cannot natively open an arbitrary linked PR beside the current PR, retain an older PR after the branch points to a newer one, or navigate a PR comment anchor into the native view.

Trigger: Try to open an arbitrary PR URL, two PRs in one thread, or retain a previous branch PR after discovering a new one.

Expected and actual: Independent PR targets should remain bound to the opened PR. Ferrite always reads the checkout's branch PR and retires other numbers. Ordinary website links become browser pages.

Cause: The PR state identity is narrower than T3's explicit repository and environment target. This is not solved safely by merely retaining more numeric tabs; the loader must also accept the selected target.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/surfaces.rs:44-45`
- `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:274-305`
- `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:319-352`
- `crates/ferrite-cockpit/src/cockpit.rs:3508-3521`
- `crates/ferrite-pane/src/preview.rs:322-331`
- `crates/ferrite-cockpit/src/cockpit/browser.rs:342-355`

T3 evidence:

- [apps/web/src/rightPanelStore.ts#L66-L80](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L66-L80)
- [apps/web/src/rightPanelStore.ts#L267-L298](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L267-L298)
- [apps/web/src/lib/openPullRequestLink.ts#L83-L134](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/openPullRequestLink.ts#L83-L134)
- [apps/web/src/components/ChatMarkdown.tsx#L1257-L1261](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1257-L1261)
- [apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1074-L1091](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1074-L1091)

Recommendation: Introduce an explicit selected PR identity containing repository, host, number, and environment or project scope. Read by that target. Retain branch PR discovery as an entry point, then allow independent selected PR tabs and anchor reveals.

Observable acceptance check: Open PR 48 and PR 49 from links in the same thread, then switch its branch. Each tab still names and loads its own PR. Equal numbers from different repositories remain distinct. A linked comment or diff anchor reveals the intended content.

Limit: No cross-project collision is claimed in the current thread-scoped model. Cross-repository identity is a requirement for expanding capability, not a demonstrated present collision.

### pr-navigation-05: A known PR without a URL still presents clickable controls that silently do nothing

Priority P2. Category `defect`. Evidence `source-proven`.

The accepted PR data shape permits a number and state without a URL. The sidebar still underlines the number and the footer appears enabled, but open_pr returns without feedback. The native PR entry could still work because it needs a number and checkout rather than the missing URL.

Trigger: Receive or restore a lightweight PR reading whose number and state exist but whose URL is absent.

Expected and actual: The visible PR action should open detail or explain unavailability. Badge hover still underlines the number; footer enables from PR existence; `open_pr` simply returns.

Cause: Availability checks disagree. The context menu and palette require a URL, but the badge and footer only require a PR reading. The parser intentionally allows absent URL.

Ferrite evidence:

- `crates/ferrite-core/src/workspace/mod.rs:539-546`
- `crates/ferrite-core/src/workspace/mod.rs:768-783`
- `crates/ferrite-cockpit/src/nav/card.rs:787-809`
- `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:324-339`
- `crates/ferrite-cockpit/src/cockpit.rs:1827-1831`
- `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:274-276`
- `crates/ferrite-cockpit/src/cockpit/context_menu.rs:460-462`

T3 comparison is not needed to establish this local availability defect.

Recommendation: Use the native PR destination when number and checkout are available. For URL-only actions, apply the same URL availability gate already used by the context menu and palette, with a clear unavailable reason.

Observable acceptance check: Render a PR reading with number 415 and url None. Its primary PR control opens native detail or clearly states why unavailable; it never appears as an enabled dead link. Normal production gh usually supplies a URL, so also verify the real failing data before attributing the user's incident to this case.

Limit: The state is accepted by production parsing and appears in a UI test fixture. This does not prove ordinary `gh pr view` returned a missing URL in the user's session. Treat it as a proven edge-case defect, not the assumed main incident.

### pr-navigation-06: The reported sidebar click failure lacks end-to-end verification at the actual entry points

Priority P2. Category `verification-gap`. Evidence `unverified`.

Existing PR tests establish the internal panel path but do not establish that the reported sidebar click works in the installed app. The source explains nonfocused and missing-URL failures, but the user may also hit a separate native browser or pointer problem when clicking a focused thread.

Trigger: Assess the user's report using existing PR tests without exercising sidebar badge and footer clicks.

Expected and actual: An event-level test should observe the visible selected destination. Existing coverage clicks the Overview row, directly invokes the + opener function, checks menu availability, and checks footer geometry.

Cause: Tests cover internal native PR state and visual existence, but the reported routes use a separate browser path. Searching the cockpit tests found no `nav-card-pr` click coverage.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/tests/pr.rs:92-134`
- `crates/ferrite-cockpit/src/cockpit/tests/pr.rs:226-247`
- `crates/ferrite-cockpit/src/cockpit/tests/nav_shelves.rs:225-239`
- `crates/ferrite-cockpit/src/cockpit/tests/nav_shelves.rs:384-418`

T3 evidence:

- [apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969)

Recommendation: Add an event-level regression test for the real sidebar badge, footer, and + menu. Have the runtime owner exercise the installed app with known URLs on focused and nonfocused threads. Assert destination visibility and selection, not just tab allocation or handler invocation.

Observable acceptance check: Record installed build identity, clicked thread, PR URL availability, focused thread, panel active tab before and after, and visible destination. Cover plain and modifier clicks and panel-hidden state. Verify a test fails if the badge handler stops activating its intended destination.

Limit: A symbol search alone cannot prove that no generic test ever touches an equivalent route. No test suite was run, and this report does not claim tests currently pass.

## Supported T3 capabilities

T3's sidebar footer enters `/pull-requests` with saved list preferences, independently of the current branch. The validated route supports involvement, state, sort, project and environment scope, repository and selected PR, query text, draft, review, checks, author, and labels. These are source-supported capabilities, not runtime-certified results. See [footer handler](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarChrome.tsx#L234-L239) and [route validation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/routes/_chat.pull-requests.tsx#L30-L78).

T3 distinguishes list scope from selected detail. Selecting a row stores its explicit project, repository, environment, host, and number in panel state and URL. The detail can select another referenced PR without changing a checkout. See [row selection](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1123-L1132) and [reference selection](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1565-L1585).

T3's badge activates another thread after a successful native panel open. A stack badge can open the thread's linked PR list. Normal PR link handling respects the app setting and modified clicks, attempts an in-app target, then falls back to link opening with an error toast. It passes supported anchors through `revealUrl`. See [badge handlers](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969), [stack handler](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1074-L1091), [target resolution](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/openPullRequestLink.ts#L74-L137), and [fallback handling](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/openPullRequestLink.ts#L243-L268).

## Positive Ferrite behavior

- Native PR detail exists, with Summary, Guide, Timeline, and Code. Its Overview row and surface menu share `open_pr_surface`, which deduplicates the tab. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:274-276`, `:452-469`; `crates/ferrite-cockpit/src/cockpit/surfaces.rs:122-134`.
- "Open a surface" in the command palette opens a hidden panel, so the native PR path does not require an already visible + button. `crates/ferrite-cockpit/src/cockpit/palette.rs:914-921`; `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:622-634`.
- PR tabs and selected subviews survive restart; stale tabs for a different discovered PR are retired. `crates/ferrite-cockpit/src/cockpit/surfaces.rs:340-363`, `:389-400`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:319-352`.
- Leaving the PR tab, hiding its panel, or showing a different thread disarms the merge confirmation. This is a useful navigation boundary. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:355-367`.
- Modifier-click on the sidebar number provides an external-browser path. The named browser commands are gated by URL. `crates/ferrite-cockpit/src/cockpit.rs:12813-12815`; `crates/ferrite-cockpit/src/cockpit/context_menu.rs:460-462`; `crates/ferrite-cockpit/src/cockpit/palette.rs:1090-1095`.
- Agent browser reveals have a user-action revision guard. A PR navigation repair should preserve that guard for agent requests while giving explicit user clicks a proper visible destination. `crates/ferrite-cockpit/src/cockpit/surfaces.rs:155-162`.
- The existing tests exercise native row opening, deduplication, subview retention, merge-state changes, and stale-number retirement. They are useful source coverage even though this worker did not run them. `crates/ferrite-cockpit/src/cockpit/tests/pr.rs:226-247`, `:250-270`, `:315-345`, `:378-410`.

## Source map

| Area | Ferrite | T3 |
| --- | --- | --- |
| Sidebar PR appearance and events | `crates/ferrite-cockpit/src/nav/card.rs:787-858`; `crates/ferrite-cockpit/src/cockpit.rs:12806-12819` | [SidebarThreadRow](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969) |
| Footer destination | `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:314-339` | [SidebarChrome](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarChrome.tsx#L234-L239) |
| Internal selected target | `crates/ferrite-cockpit/src/cockpit/surfaces.rs:25-91`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:274-305` | [rightPanelStore](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L267-L298) |
| Visible destination selection | `crates/ferrite-cockpit/src/cockpit/panel.rs:181-211`; `crates/ferrite-cockpit/src/cockpit/browser.rs:637-647` | [PR link routing](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/openPullRequestLink.ts#L74-L137) |
| Project list scope versus detail selection | No standalone PR list route found; current thread checkout is used | [usePullRequestListView](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestListView.ts#L89-L160) |
| Alternative review routes | `crates/ferrite-cockpit/src/review.rs:796-805`, `:2378-2384`; `crates/ferrite-cockpit/src/cockpit/delivery.rs:108-122` | [PullRequestsPage detail](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1552-L1585) |

## Verification gaps and cross-slice boundaries

The runtime owner should first click a known valid PR on the focused thread, then the same control on a nonfocused thread, with the panel both shown and hidden. Record whether a Browser tab, native PR tab, external browser, or no visible destination appears. Then select the previously clicked thread to test whether its earlier browser request becomes visible. Repeat from the empty board. This discriminates destination routing from a native browser startup failure or a pointer interception problem.

The runtime owner should exercise the Overview row, real + menu item, palette surface menu, footer, and modifier-click independently. Existing internal methods do not validate those actual mouse paths. No screenshot or installed build verification was available to this worker.

PR loading and authentication affect reachability. `gh pr view` errors, missing authentication, and no branch PR all collapse to no lightweight reading in `crates/ferrite-core/src/workspace/mod.rs:724-749`; the + menu then says "No pull request on this branch" at `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:142-144`. The PR operations report owns that diagnosis as `pr-operations-003`, including why lightweight refresh does not recover a cached snapshot error. This report does not count it again.

The native header's URL uses facts while the breadcrumb can use the loaded snapshot. The source also returns early when a lightweight reading disappears, leaving snapshot lifecycle questions. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:324-335`, `:492-532`. Stale-data and action safety belong to PR operations; no additional wrong-PR mutation is asserted here.

This report proves no unsolicited thread focus change from PR refresh or browser reveal. It proves the inverse for explicit clicks: the required target activation is missing. Agent-driven background routing belongs to the background-work worker. Keyboard focus, pointer hit-testing, and embedded-browser focus behavior belong to the input and browser workers.

The badge already changes underline on hover, using `hover_snap`, at `crates/ferrite-cockpit/src/nav/card.rs:790-807`. The missing outcome is a visible destination, so adding animation alone would not repair it. The user's broader motion request remains valid and belongs to the motion report.

Recommended sequence: repair the explicit click destination and add its event-level regression; align native PR entry points; make missing-URL states honest; then design an explicit PR target before adding a standalone inbox or multiple native PR tabs. This ordering fixes the reported dead navigation without making the current branch-bound loader carry targets it cannot represent.
