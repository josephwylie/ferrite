# PR operations investigation

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants a detailed account of broken PR workflows and missing T3 capabilities so Ferrite can become reliable for everyday review and delivery. This report traces PR creation, loading, checks, discussion, reviews, mutations, refresh, authentication and worker lifecycle. Navigation and external-link reachability belong to the separate `pr-navigation` report.

## Overview

Status is ISSUES. The new PR panel has concrete source-proven defects in refresh, discussion rendering, guide selection, creation availability and recovery. It also implements a much smaller review workflow than current T3. The strongest defects are not dependent on aesthetic preference or on whether native forge writes are desired.

Ferrite intentionally delegates creation and state-changing actions to an agent. That is a product choice, not proof that every operation fails. The material gap is that its panel lacks an operation lifecycle, reliable post-action refresh, and target identity in most generated prompts. A capable agent can compensate for some missing operations through chat, but that does not make the visible controls equivalent to native review tools.

## Method and source pins

- Ferrite baseline is `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. The checkout HEAD matched this commit.
- T3 baseline is `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, inspected at `/tmp/ferrite-t3-audit-source-20261113`. T3 links below are commit-pinned.
- This is a read-only source investigation. No application edits, forge writes, live UI actions, tests or builds were performed. Tests cited below were inspected, not executed.
- The investigation followed event handlers, cache changes, rendering, wire conversion, forge reads, CLI process execution and T3 provider mutation calls. Searches covered the full cockpit/core source for alternate native review operations. Factory task approval is not a GitHub review verdict.
- ADR 0039 establishes the new tabbed host while retaining the older Review slot. Both implementations were inspected so older working behavior was not mistaken for missing repository-wide capability. ADRs 0038 and 0040 provide UI-test and navigation context; historical T3 specs were not treated as proof of current T3 behavior.
- Throughput checkpoint is n/a, read-only investigation. No delegates were started, as the assigned brief prohibits spawning.

## Key concepts

Ferrite has two PR readings. The lightweight workspace reading uses `gh pr view` for navigation facts. The detailed reader resolves a forge repository and branch, then returns `ReviewPullRequest`, which contains a `WirePr` or an error note plus a structured unread reason. The panel reduces that to `PrRead::Reading` or one result in a map keyed by thread.

A PR action in this panel is usually a prompt sent to the thread's agent. It has no native forge-operation result object. The agent may run immediately or queue behind a turn. The new panel and older Review slot both consume `WirePr`, but their rendering and refresh paths differ.

T3 has an explicit host/repository/PR reference, capability and permission data, separate detail/activity/check queries, typed mutations and UI pending/error handling. Its generated guide is a head-specific structured result rather than a convention in comment text.

## Traced flows

| Flow | Ferrite path and outcome | T3 comparison |
| --- | --- | --- |
| Create | Transcript-derived changed files expose Create PR. The button sends a commit/push/gh-create prompt. | Git status determines availability; GitManager generates text and executes a provider creation phase. |
| Load | The opener forces a new detailed read. A detached task writes one thread-keyed result. | Detail, activity and checks use a canonical PR reference and separate queries. |
| Refresh | Refresh PR status rechecks the lightweight reading. Detail invalidates only on number/tone changes. | Live detail/check refresh and explicit host invalidation refresh detail/activity. |
| Mutation | Menu availability uses tone; an agent receives a prompt. The panel has no pending result. | Action runner deduplicates, tracks pending, reports error/success and invokes result handling. |
| Discussion | The backend reads review roots, replies, resolution and quoted hunks; the new Timeline drops most of them. | Review threads, replies, verdicts, reactions and native mutation paths are represented. |
| Guide | A comment prefix identifies a guide, then reversed order selects the oldest matching retained comment. | Structured guide stores head SHA, stops and run status, with stale/failure feedback. |
| Recovery | Basic discovery errors disappear as None; detail errors reuse an r-retry instruction without a new-panel retry action. | Provider errors distinguish auth, unavailable credentials and rate limits; panel exposes explicit refresh/retry paths. |
| Worker | One-shot detail reads use a false cancellation flag and no deadline. | GitHub request policy applies a default timeout and concurrency limit. |

The exact sources for every row appear in the findings below.

## Source map

- `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:99-116` defines the action set. Lines 159-205 format prompts, 283-335 own snapshot loading/invalidation, and 689-840 render Summary/Guide/Timeline.
- `crates/ferrite-cockpit/src/cockpit/panel/changes.rs:19-45` determines creation visibility. Lines 119-134 send the prompt and 296-319 define it.
- `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:358-380` sends or queues the handoff.
- `crates/ferrite-core/src/workspace/mod.rs:719-760` owns lightweight gh discovery.
- `crates/ferrite-core/src/protocol/review.rs:478-505` defines the snapshot. Lines 575-610 define discussion data and 689-747 define detailed read failures.
- `crates/ferrite-reviewr/src/ferrite.rs:649-664` adapts the one-shot detail read. `crates/ferrite-reviewr/src/forge.rs:338-352` executes the provider process, 853-865 projects GitHub data, and 1025-1130 normalizes discussion.
- `crates/ferrite-cockpit/src/review.rs:1898-1953` is the older Review slot's discussion rendering. Its replies/status/truncation behavior remains useful comparison evidence.
- T3 detail hooks, mutation runner, GitHub provider and guide contract are linked per finding. These source-level implementations are not evidence that the user's installed T3 or Ferrite binary was exercised.

## Detailed findings

### pr-operations-001 · PR detail stays stale after status refresh and agent actions

P1. defect. Evidence is source-proven.

Symptom and impact. Users cannot trust the displayed checks, comments, description, or generated guide to reflect the current PR. A running check can remain running after completion.

Trigger. Open a PR, change checks or comments without changing its number or open/draft/merged/closed state, then use Refresh PR status. The same problem follows Write guide.

Expected. Refresh updates the visible detail and activity. A completed PR operation invalidates the data it changed.

Actual. The status command refreshes only the lightweight reading. Detail invalidation compares only number and tone. The existing snapshot blocks another passive read.

Cause. Two PR caches have different refresh paths. read_pr_snapshot(false) returns whenever any result exists. pr_reading_changed ignores all changes except number and tone, and returns immediately for no reading.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:283-305`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:319-335`; `crates/ferrite-cockpit/src/cockpit/palette.rs:1302-1305`; `crates/ferrite-cockpit/src/cockpit.rs:1813-1823`; `crates/ferrite-cockpit/src/cockpit/panel.rs:254-266`.

T3 evidence. [apps/web/src/components/pullRequest/usePullRequestDetail.ts:65-84](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L65-L84); [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:624-676](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L624-L676).

Recommendation. Give detail a refresh command and invalidation keyed by repository, PR and head/revision. Refresh activity after writes and checks while pending. Preserve a timestamp and stale state.

Observable acceptance check. Open a PR with a pending check, finish the check remotely, add a comment, and invoke Refresh PR status. The same panel shows the new result and comment. A new guide appears after its agent completes without reopening the PR.

### pr-operations-002 · Reload discards the last good PR and replaces it with a transient error

P2. defect. Evidence is source-proven.

Symptom and impact. A temporary network or authentication failure removes previously readable review context, including access to the Code tab body.

Trigger. Reopen an already-loaded PR through its opener while offline or while the forge API is failing.

Expected. Keep the last good snapshot visible, mark it stale, show the failure, and offer retry.

Actual. The read immediately replaces the stored snapshot with Reading. Its completion replaces that with Err. Every tab then renders the error note instead of its body.

Cause. PrRead stores either loading or one result. read_pr_snapshot drops ReviewPullRequest.unread, even though the protocol distinguishes transient failures and documents preservation of the last snapshot.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:32-36`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:293-305`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:461-469`; `crates/ferrite-core/src/protocol/review.rs:689-708`.

T3 evidence. [apps/web/src/components/pullRequest/usePullRequestDetail.ts:56-74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L56-L74); [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:624-672](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L624-L672).

Recommendation. Store last-success, pending and error separately. Preserve content on retriable failures and clear it only when authoritative identity or absence changes.

Observable acceptance check. Load a PR, simulate a failed refresh, and verify that title, discussion and diff remain visible with an actionable stale/error indicator. Retry succeeds without closing the tab.

### pr-operations-003 · Authentication and retry guidance do not form a working recovery flow

P2. defect. Evidence is source-proven.

Symptom and impact. Users see no PR or a dead-end instruction instead of a recoverable authentication error. Missing CLI, expired credentials and a branch with no PR look identical during discovery.

Trigger. Discover a PR without a working gh login, or receive a detail failure after opening a previously discovered PR.

Expected. Distinguish no PR, missing CLI, unauthenticated host and temporary failure. A visible retry control must retry the failed detail request.

Actual. Discovery maps all gh failures to None. Detailed errors say to press r, but the new PR surface renders a plain note with no retry handler. The existing Refresh PR status action only refreshes discovery, leaving cached detail errors intact.

Cause. The new surface reuses the older review pane error text, which assumes its r shortcut, but drops the structured unread field and has no corresponding recovery action.

Ferrite evidence. `crates/ferrite-core/src/workspace/mod.rs:724-749`; `crates/ferrite-core/src/protocol/review.rs:732-747`; `crates/ferrite-reviewr/src/forge.rs:44-75`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:294-305`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:867-878`; `crates/ferrite-cockpit/src/cockpit.rs:10483-10486`; `crates/ferrite-ui/src/keymap.rs:631`.

T3 evidence. [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:64-69](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L64-L69); [packages/source-control-github/src/server/GitHubRequestPolicy.ts:36-64](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubRequestPolicy.ts#L36-L64); [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:624-676](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L624-L676).

Recommendation. Propagate typed discovery failures to the PR opener and panel. Add a retry button bound to detail reload and make the displayed shortcut match the real binding. Keep host-specific login commands.

Observable acceptance check. With a mocked missing executable and then an expired login, show distinct remedies. After restoring authentication, clicking Retry must load the PR. The documented keyboard action must do the same.

### pr-operations-004 · Rewrite guide displays the oldest matching guide

P2. defect. Evidence is source-proven.

Symptom and impact. A rewritten guide can appear not to have changed, and reviewers can read superseded instructions.

Trigger. A PR contains two retained comments beginning with ## Review guide.

Expected. Guide shows the newest matching comment.

Actual. The backend returns comments newest first. guide reverses that order before finding the first match, selecting the oldest guide.

Cause. The panel assumes the opposite chronological order from the forge normalization. WirePr conversion preserves backend order.

Ferrite evidence. `crates/ferrite-reviewr/src/forge.rs:1025-1028`; `crates/ferrite-core/src/protocol/review.rs:652-678`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:224-235`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:749-756`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestGuideTab.tsx:89-99](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestGuideTab.tsx#L89-L99); [packages/contracts/src/pullRequestReviewGuide.ts:45-69](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/pullRequestReviewGuide.ts#L45-L69).

Recommendation. Select the newest by timestamp or honor the canonical newest-first contract. Add a fixture with multiple guides in real backend order.

Observable acceptance check. With older and newer guide comments from a human account, Guide displays the newer one. Rewriting and refreshing must replace it with the latest text.

### pr-operations-005 · Timeline reverses the backend order and has no ordering control

P3. defect. Evidence is source-proven.

Symptom and impact. New activity appears at the bottom of a long discussion despite the panel documenting newest-first behavior.

Trigger. Open Timeline on a PR with two or more dated comments.

Expected. Use the declared newest-first default and expose a deliberate ordering choice if both orders are needed.

Actual. The newest-first backend list is iterated in reverse, so the oldest entry renders first.

Cause. pr_timeline applies rev to an already descending list.

Ferrite evidence. `crates/ferrite-reviewr/src/forge.rs:1025-1028`; `crates/ferrite-core/src/protocol/review.rs:652-678`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:789-795`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx:753-797](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx#L753-L797).

Recommendation. Remove the accidental reversal or define a shared order contract and add a user preference.

Observable acceptance check. With comments at 09:00 and 10:00, the default Timeline begins with 10:00. Any oldest-first option visibly changes the order.

### pr-operations-006 · Timeline drops review replies, resolution state and quoted code

P2. defect. Evidence is source-proven.

Symptom and impact. Reviewers cannot see answers or distinguish open findings from already resolved or outdated discussions. A finding loses the code context needed to evaluate it.

Trigger. Open Timeline for a review thread containing replies, a diff hunk, and resolved/outdated flags.

Expected. Show the thread root, its replies, resolution and outdated state, and quoted code or an anchored diff link.

Actual. Only author, a text anchor, timestamp and root body render. The supplied replies, snippet, resolved and outdated values are never consumed.

Cause. The new rendering function projects away data already supplied on WirePrComment. The older Review pane displayed replies and status, so this is a regression in the new PR route.

Ferrite evidence. `crates/ferrite-core/src/protocol/review.rs:575-610`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:789-840`; `crates/ferrite-cockpit/src/review.rs:1898-1953`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx:462-543](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx#L462-L543); [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:620-631](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L620-L631).

Recommendation. Render complete review threads and their state. Reuse the canonical thread model for Summary, Timeline and inline code comments.

Observable acceptance check. A fixture with a resolved, outdated thread and two replies exposes all four facts. The user can open the associated code location and distinguish it from an unresolved thread.

### pr-operations-007 · Capped PR data is presented as a complete conversation and check result

P2. defect. Evidence is source-proven.

Symptom and impact. A reviewer can mistake a partial list for all findings or all checks. Missing failure data can make the summary appear more reassuring than the complete PR.

Trigger. A PR exceeds a fetched connection cap, such as 100 check contexts or 100 review threads.

Expected. Show which data is incomplete and provide pagination or an explicit forge fallback. Avoid a definitive passing rollup when completeness is unknown.

Actual. The backend records separate checks/comments truncation flags. The new PR panel ignores them and labels a nonempty subset with no failures/running checks as Checks passing.

Cause. The UI does not use WirePr.more_checks or more_comments. The older Review pane at least showed More on the forge.

Ferrite evidence. `crates/ferrite-reviewr/src/forge.rs:853-865`; `crates/ferrite-reviewr/src/forge.rs:964-974`; `crates/ferrite-core/src/protocol/review.rs:496-517`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:246-263`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:689-745`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:789-840`; `crates/ferrite-cockpit/src/review.rs:1952-1953`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:1219-1221](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L1219-L1221); [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:620-623](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L620-L623).

Recommendation. Show explicit partial-data notices separately for checks and discussion. Page remaining data where supported and withhold a complete-pass claim until coverage is known.

Observable acceptance check. Inject checks_truncated and comments_truncated independently. Each produces its own notice and route to more data. A truncated success-only subset never claims all checks pass.

### pr-operations-008 · Checks stop at names and icons without a route to failure details

P2. partial-capability. Evidence is source-proven.

Symptom and impact. A failed job requires manually finding the run on the forge. Users cannot inspect the result from the displayed check or distinguish some important status categories.

Trigger. A PR has a failing, cancelled, action-required, or timed-out CI check.

Expected. Expose the check details URL, readable status, and relevant description. Preserve distinctions needed to choose the next action.

Actual. WireCheck carries only name/status and the panel renders inert rows. The backend collapses cancelled, timed-out and action-required completed runs to Failure.

Cause. The query omits check URLs and descriptions, the wire model cannot carry them, and the render has no click handler.

Ferrite evidence. `crates/ferrite-reviewr/src/forge.rs:859-860`; `crates/ferrite-reviewr/src/forge.rs:1047-1064`; `crates/ferrite-core/src/protocol/review.rs:557-572`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:702-716`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx:55-109](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx#L55-L109); [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:1263-1279](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L1263-L1279).

Recommendation. Carry check URLs and useful status distinctions through the model. Add a Details action with visible open failure handling.

Observable acceptance check. A failed job opens its own run page from the PR panel. Cancelled and approval-required runs have distinct labels and appropriate next actions.

### pr-operations-009 · Descriptions, comments and guides render as raw text

P2. partial-capability. Evidence is source-proven.

Symptom and impact. PR templates, code fences, tables, checklists, image evidence and linked documents lose their intended presentation and interaction.

Trigger. Open a PR body, comment or guide with Markdown links, headings, fenced code, a table, or an image.

Expected. Render supported forge Markdown with working links and code formatting. Offer accessible text selection and copy where supported.

Actual. All three views insert SharedString directly. There is no Markdown renderer or link handling on those bodies.

Cause. The PR panel uses plain GPUI text instead of the existing rich document rendering architecture.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:727-733`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:752-756`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:832-837`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestMarkdown.tsx:17-28](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestMarkdown.tsx#L17-L28); [apps/web/src/components/pullRequest/PullRequestMarkdown.tsx:30-113](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestMarkdown.tsx#L30-L113); [apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx:647-663](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx#L647-L663).

Recommendation. Use a shared Markdown component with forge-relative links, image handling, code copy and safe task-list interaction.

Observable acceptance check. An example PR with a linked issue, fenced code, table, checklist and image renders each correctly. Links open their actual destinations and long code remains usable.

### pr-operations-010 · No native comment, reply, review verdict or resolution workflow

P2. missing-capability. Evidence is source-proven.

Symptom and impact. Users must leave the PR pane or ask an agent to conduct the review. They cannot prepare a batch of inline comments and submit an approval or request changes in the UI.

Trigger. Try to comment, reply to a thread, edit a comment, react, resolve/reopen a discussion, approve, or request changes from the PR panel.

Expected. Provide these operations if full T3 review parity is a product goal, with permission checks, retained drafts and operation-specific errors.

Actual. The complete action enum has handoffs, lifecycle actions and copy actions only. Timeline is read-only. Searches across cockpit/core found no native forge review submission path; Factory approval is a separate workflow and does not count.

Cause. The new PR implementation is a reader with agent handoffs, without comment/review mutation commands or stable comment identifiers in WirePrComment.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:99-116`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:429-449`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:789-840`; `crates/ferrite-core/src/protocol/review.rs:575-610`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestReviewForm.tsx:27-48](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestReviewForm.tsx#L27-L48); [apps/web/src/components/pullRequest/PullRequestReviewForm.tsx:100-148](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestReviewForm.tsx#L100-L148); [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:755-805](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L755-L805).

Recommendation. Add typed, permission-aware native review operations or explicitly label the limited workflow and provide reliable forge handoff. Keep failed review drafts.

Observable acceptance check. Compose an inline comment and approval, fail the submit once, retry without losing text, then see the posted review. Reply and resolve a thread and verify the new state after refresh.

### pr-operations-011 · PR metadata cannot be edited or managed in the native panel

P2. missing-capability. Evidence is source-proven.

Symptom and impact. Users cannot correct a title or description, request reviewers, or apply labels while reviewing in Ferrite.

Trigger. Try to edit PR title/body, add/remove a reviewer request, or change labels.

Expected. Provide capability-aware metadata editing and preserve edits on failure if matching T3 is intended.

Actual. Title and body are plain text. WirePr has no author, labels, reviewer collection or edit permissions, and the action enum has no metadata editing operation.

Cause. The snapshot and panel contract omit these data and commands.

Ferrite evidence. `crates/ferrite-core/src/protocol/review.rs:478-505`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:99-116`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:717-745`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx:647-663](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx#L647-L663); [apps/web/src/components/pullRequest/PullRequestReviewerPicker.tsx:66-107](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestReviewerPicker.tsx#L66-L107); [apps/web/src/components/pullRequest/PullRequestLabelPicker.tsx:35-51](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestLabelPicker.tsx#L35-L51); [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:740-753](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L740-L753).

Recommendation. Extend the domain model and add focused editors with explicit saving/error states and permission checks.

Observable acceptance check. Edit title/body and change labels/review requests without leaving the panel. A rejected save retains the draft and shows the reason.

### pr-operations-012 · Agent-backed PR operations have no panel-level pending, success or failure lifecycle

P2. partial-capability. Evidence is source-proven.

Symptom and impact. Clicking Merge, Ready, Write guide or Create PR can appear to do nothing while work queues behind another turn. Repeated clicks can enqueue repeated requests.

Trigger. Run a PR action while the thread is already busy, or run an action whose gh command fails.

Expected. Label the agent handoff, show queued/running/failed/completed state for that operation, prevent duplicate submissions, and refresh changed PR data after completion.

Actual. run_pr_action sends or queues a prompt and returns. The panel stores no operation identifier or pending action. The primary action resets its confirmation state immediately. Create PR follows the same path.

Cause. Agent handoff is an explicit architectural choice, but its lifecycle is not represented in the PR UI. Completion and error information is only whatever the transcript reports.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:9-15`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:429-449`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:593-602`; `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:358-380`; `crates/ferrite-cockpit/src/cockpit/panel/changes.rs:119-134`.

T3 evidence. [apps/web/src/components/pullRequest/usePullRequestActions.ts:127-186](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L127-L186); [apps/server/src/git/GitManager.ts:2457-2497](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/git/GitManager.ts#L2457-L2497).

Recommendation. Keep agent execution if desired, but model an operation with a target, state and result. Show that state beside the initiating control and deduplicate repeated requests.

Observable acceptance check. Click an action while a turn is busy. Show Queued and the target PR, block duplicate enqueue, then transition through Running to success/failure and refresh the affected data.

### pr-operations-013 · Lifecycle actions lack inverse operations and branch maintenance

P2. missing-capability. Evidence is source-proven.

Symptom and impact. A user can enable auto-merge or close a PR but cannot undo those choices from the same UI. Updating a stale branch and approving blocked workflows also require leaving the panel.

Trigger. Open a closed PR, an auto-merge-enabled PR, a branch behind its base, or a PR awaiting workflow approval.

Expected. Offer reopen, disable auto-merge, update branch and approve workflows where the host permits them. Maintainer edits should be a true on/off state.

Actual. The enum exposes Close and AutoMerge only, never Reopen, DisableAutoMerge, UpdateBranch or ApproveWorkflows. AllowEdits always sends maintainer_can_modify=true and displays no current value.

Cause. PR state omits auto-merge and maintainer-edit state, and the action set implements only one side of several transitions.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:99-116`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:147-155`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:189-197`; `crates/ferrite-core/src/protocol/review.rs:478-505`.

T3 evidence. [apps/web/src/components/pullRequest/usePullRequestActions.ts:79-104](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L79-L104); [apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx:288-311](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L288-L311); [apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx:341-344](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L341-L344); [apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx:434-440](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L434-L440).

Recommendation. Model the current settings and complete reversible lifecycle actions. Gate them by actual host capability and permission.

Observable acceptance check. Enable then disable auto-merge, close then reopen a PR, toggle maintainer edits both ways, and update an eligible branch. Each result appears after refresh.

### pr-operations-014 · PR action prompts lose canonical host and repository identity

P2. partial-capability. Evidence is inferred.

Symptom and impact. For forks, multiple remotes, enterprise hosts or non-GitHub forges, the agent must reconstruct the target and may fail or act in the wrong repository. No wrong-target mutation was performed in this audit.

Trigger. A read resolves a PR through an upstream or pinned repository, then a mutation prompt uses only its number. Delete branch uses origin regardless of the PR source repository.

Expected. Every operation should carry the canonical provider, host, repository, PR number and head repository/ref. Execution should verify that identity.

Actual. Only Explain includes pr.url. Other operations use gh pr commands with a bare number. DeleteBranch hardcodes git push origin --delete. The read kernel supports GitLab and Azure DevOps but these prompts always use GitHub commands.

Cause. WirePr loses the forge type and fork source repository. The action formatter uses only number/head and assumes gh/origin.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:159-205`; `crates/ferrite-core/src/protocol/review.rs:612-626`; `crates/ferrite-reviewr/src/forge.rs:522-540`; `crates/ferrite-reviewr/src/ferrite.rs:649-659`.

T3 evidence. [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:722-780](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L722-L780); [packages/source-control-core/src/server/PullRequestProvider.ts:495-515](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-core/src/server/PullRequestProvider.ts#L495-L515).

Recommendation. Carry an explicit target identity through every operation. For agent handoffs, include the URL and provider-specific command plus a target-verification step. Resolve branch deletion from the PR head repository.

Observable acceptance check. With origin=fork and upstream=base, generated operations target the named PR and correct remote branch. GitLab/Azure reads generate matching provider operations or clearly unavailable controls.

### pr-operations-015 · Concurrent PR reads can land out of order and retire the current tab

P2. defect. Evidence is inferred.

Symptom and impact. A slow old response can overwrite newer details, or remove the tab for the PR now on the branch. The source permits this race; timing was not reproduced.

Trigger. Open the PR repeatedly while a read is pending, or switch the branch/PR between overlapping requests.

Expected. Only the newest request for the current repository/branch/PR identity may update state or retire tabs.

Actual. Forced opener reads ignore the existing request. Every detached completion writes the thread-keyed cache unconditionally and calls retire_other_prs using the returned number.

Cause. The cache has a ThreadId key, no request generation or captured target check, and no cancellation handle.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel.rs:81`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:283-309`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:338-351`; `crates/ferrite-cockpit/src/cockpit/panel.rs:254-266`.

T3 evidence. [apps/web/src/components/pullRequest/usePullRequestDetail.ts:49-68](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L49-L68); [apps/web/src/components/pullRequest/usePullRequestActions.ts:144-160](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L144-L160).

Recommendation. Capture canonical target and request generation, deduplicate pending reads, and reject stale completions before they modify cache or surfaces.

Observable acceptance check. Delay read A, start read B for a different current PR, complete B then A. B stays visible and its tab remains. Repeated open of one pending PR creates one request.

### pr-operations-016 · PR read workers have no effective cancellation or deadline

P2. defect. Evidence is source-proven.

Symptom and impact. A hung forge process can leave Reading visible indefinitely and retain a worker after the pane is closed. Repeated forced opens can accumulate stuck reads.

Trigger. A forge CLI hangs during a detail read, then the user closes/reopens the PR or changes its branch.

Expected. Cancel superseded reads and stop their process trees. Bound remote reads with a deadline and show a retriable failure.

Actual. The UI detaches its task. The one-shot backend calls forge::fetch, which creates a permanently false cancellation flag. run_provider has no deadline and waits until that flag changes or the process exits.

Cause. The lower-level process runner supports cancellation, but this adapter never supplies an externally controllable signal. Server and in-process requests each spawn a thread.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:294-309`; `crates/ferrite-reviewr/src/ferrite.rs:652-659`; `crates/ferrite-reviewr/src/forge.rs:338-352`; `crates/ferrite-reviewr/src/forge.rs:484-485`; `crates/ferrite-core/src/server.rs:1118-1129`; `crates/ferrite-core/src/cockpit.rs:1251-1259`.

T3 evidence. [packages/source-control-github/src/server/GitHubRequestPolicy.ts:21-24](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubRequestPolicy.ts#L21-L24); [packages/source-control-github/src/server/GitHubRequestPolicy.ts:427-434](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubRequestPolicy.ts#L427-L434).

Recommendation. Thread cancellation and a deadline through the one-shot adapter; cancel on supersession and release the process tree. Share equivalent safeguards with badge discovery.

Observable acceptance check. A fake gh that never exits reaches a visible timeout. Closing or superseding the request terminates its process tree, and repeated retries do not leave worker processes behind.

### pr-operations-017 · Review verdicts and empty-body approvals are absent from the snapshot

P2. partial-capability. Evidence is source-proven.

Symptom and impact. Reviewers cannot tell who approved, requested changes, or had a review dismissed. An approval with no written body disappears entirely.

Trigger. A teammate submits an approval without text, or a request-changes verdict that the user must distinguish from a normal comment.

Expected. Keep verdict, author, submitted commit/time and dismissal/staleness state. Show verdict-only reviews.

Actual. The GraphQL review projection omits state, and merge_comments drops reviews with empty body. WirePrComment records only generic Review kind.

Cause. The model treats reviews as prose comments and loses decision semantics before rendering.

Ferrite evidence. `crates/ferrite-reviewr/src/forge.rs:861`; `crates/ferrite-reviewr/src/forge.rs:1072-1078`; `crates/ferrite-core/src/protocol/review.rs:575-603`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:795-804`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx:522-542](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx#L522-L542); [apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx:723-747](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx#L723-L747); [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:600-609](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L600-L609).

Recommendation. Preserve structured review outcomes, including empty-body verdicts and whether the review applies to the latest head.

Observable acceptance check. An approval with an empty body appears with author and verdict. A later push marks an old review appropriately. Changes requested and dismissed reviews remain distinguishable.

### pr-operations-018 · PR panel omits the checkout-versus-host sync warning

P2. defect. Evidence is source-proven.

Symptom and impact. Users may confuse local changes shown in Code with what reviewers can actually see on the host, because the PR view drops the unpushed/behind warning.

Trigger. Open a PR when the local checkout has unpushed commits or is behind its hosted head.

Expected. Show the relationship between local checkout and PR head wherever local code is displayed as the PR Code view.

Actual. WirePr carries sync, but Summary uses only head/base and mergeability. Code uses the local branch changes reader. The older Review pane displayed unpushed/behind counts.

Cause. The new summary omits a meaningful field from the existing model.

Ferrite evidence. `crates/ferrite-core/src/protocol/review.rs:490-491`; `crates/ferrite-core/src/protocol/review.rs:540-554`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:466`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:689-701`; `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:71-84`; `crates/ferrite-cockpit/src/review.rs:2519-2541`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestCodeTab.tsx:435-464](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L435-L464); [apps/web/src/components/pullRequest/PullRequestCodeTab.tsx:649-652](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L649-L652).

Recommendation. Restore the sync notice and explicitly identify which revision Code displays. Prefer the hosted PR revisions for hosted review operations.

Observable acceptance check. With two unpushed commits, the PR panel says so and identifies the displayed code revision. With a behind checkout, it does not silently present the local diff as the current hosted PR.

### pr-operations-019 · Guide is a comment convention without revision-aware review workflow

P2. partial-capability. Evidence is source-proven.

Symptom and impact. A guide can remain apparently current after a push, and its file references cannot act as a structured review tour. Guide generation has no local run or failure state.

Trigger. Generate a guide, push a new head, or ask for a guide that fails midway.

Expected. For T3 parity, associate the guide with a head SHA, show running/failed/stale state, and connect guide stops to actual code excerpts and files.

Actual. Ferrite asks the agent to post a comment beginning ## Review guide, then displays the matching comment as raw text. The panel cannot know which revision it describes.

Cause. The guide is inferred from comment prose and the snapshot omits head SHA. There is no structured guide result/run model.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:176-179`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:224-235`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:749-784`; `crates/ferrite-core/src/protocol/review.rs:478-505`.

T3 evidence. [packages/contracts/src/pullRequestReviewGuide.ts:12-69](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/pullRequestReviewGuide.ts#L12-L69); [apps/web/src/components/pullRequest/PullRequestGuideTab.tsx:89-145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestGuideTab.tsx#L89-L145).

Recommendation. Decide whether a comment guide is an intentional reduced feature. At minimum record head identity, generation state and stale status. Full parity needs ordered stops with linked excerpts.

Observable acceptance check. After generating a guide, push a new commit. The guide is visibly stale. A failed rewrite retains the old guide and offers retry; successful stops open the corresponding file and lines.

### pr-operations-020 · Create PR is unavailable for branch work absent from the current thread transcript

P1. defect. Evidence is source-proven.

Symptom and impact. A user who opens a prepared branch, makes manual edits, or imports work from another session cannot use the native Create PR button unless this thread has recorded file edits.

Trigger. Open a branch with commits ready to publish and no PR, in a new thread that has not emitted file-change instruments.

Expected. PR creation availability should follow repository branch/diff/push state, regardless of which agent or tool made the changes.

Actual. The entire Changes section returns None when thread changed_files is empty and no PR exists. Create PR is only attached inside the nonempty changed_files summary.

Cause. changed_files is a transcript instrument digest, not a repository diff. The creation entry point uses that unrelated visibility condition.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/changes.rs:19-45`; `crates/ferrite-cockpit/src/cockpit/panel/changes.rs:114-133`; `crates/ferrite-cockpit/src/facts.rs:85-87`; `crates/ferrite-core/src/digest.rs:355-376`.

T3 evidence. [apps/web/src/components/GitActionsControl.logic.ts:117-136](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/GitActionsControl.logic.ts#L117-L136); [apps/web/src/components/GitActionsControl.logic.ts:164-174](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/GitActionsControl.logic.ts#L164-L174).

Recommendation. Offer Create PR from repository state or a stable command, independent of transcript-derived change cards. Preserve the current agent handoff if that is the chosen creation design.

Observable acceptance check. Open a fresh thread on a branch with an ahead-of-base commit created outside Ferrite. Create PR is discoverable and targets that branch. No synthetic agent edit is required.

### pr-operations-021 · Merge controls ignore permissions, allowed methods and readiness

P2. partial-capability. Evidence is source-proven.

Symptom and impact. The panel offers actions the user cannot perform and always presents Merge on an open PR, even with conflicts or blocked checks. The user cannot choose a supported merge method.

Trigger. Open a read-only repository, conflicting PR, or repository that allows only a specific merge strategy.

Expected. Derive available actions from host capabilities, viewer permissions, mergeability and allowed methods. Explain unavailable actions.

Actual. PrAction.applies checks only PR tone. pr_primary chooses Merge for any non-draft, non-settled PR. The prompt asks the agent to infer the default merge method, and WirePr has no permissions or allowed methods.

Cause. The model lacks the data needed for accurate action availability. Server-side branch protections still apply; this is misleading UI, not evidence of a protection bypass.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:147-155`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:185-191`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:568-601`; `crates/ferrite-core/src/protocol/review.rs:478-505`.

T3 evidence. [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:1267-1291](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L1267-L1291); [apps/web/src/components/pullRequest/usePullRequestActions.ts:49-76](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L49-L76); [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:76-105](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L76-L105).

Recommendation. Fetch viewer permissions, available merge methods and readiness. Disable or relabel unavailable actions, let users select a method, and bind confirmation to the intended target/revision.

Observable acceptance check. For a read-only viewer there is no enabled Merge. A conflicting PR displays the blocker. A squash-only repository offers the correct method and the operation reports its actual result.

## Supported T3 capabilities

The comparison establishes native action execution with pending/error/success feedback; permission-aware merge controls and merge methods; reopen, auto-merge disable and branch update; title/body editing; labels and reviewer requests; comment creation/edit, review submission, replies, reactions and resolution; structured review verdicts; live checks and check-detail links; Markdown and media rendering; and guides tied to a head SHA with stale/run/failure state. The evidence is linked in findings 001 and 008 through 021. These capabilities are not all mandatory Ferrite requirements. Product parity choices should be explicit, while defects in advertised Ferrite behavior should be fixed independently.

## Positive Ferrite behavior

The PR reader is not a stub. Its backend resolves fork and pinned repository identities, routes GitHub/GitLab/Azure reads, maps missing CLI and authentication failures into remedies, caps large data with explicit flags, completes pages of replies within selected review threads, and retains resolution/outdated/snippet data. Relevant sources are `crates/ferrite-reviewr/src/forge.rs:44-75`, `:522-540`, `:884-938`, `:964-995` and `:1090-1126`.

The UI has a real two-press merge confirmation, disarms it when the surface is no longer visible, hides actions inappropriate for merged/closed states, retains the selected PR subtab, and preloads Code near hover. These behaviors are defined in `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:147-155`, `:355-367`, `:387-400`, `:593-602` and `:637-645`.

The tests inspect real UI clicks for merge confirmation and exercise state transitions. `crates/ferrite-cockpit/src/cockpit/tests/pr.rs:280-312` checks that two clicks send one merge prompt. Lines 318-345 check a merged reading changes the primary control. Lines 350-365 begin the leave-and-disarm scenario. `crates/ferrite-cockpit/src/cockpit/tests/review.rs:553-580` covers Create PR sending a prompt. These establish useful intent and regression protection, but do not validate a live forge mutation or its completion state.

The creation prompt checks for an existing PR and asks for its URL rather than always creating another. It requests a PR template and considers the default branch. This is explicit in `crates/ferrite-cockpit/src/cockpit/panel/changes.rs:296-319`.

Server reads run off the main pump, and the lower-level process runner can kill a process tree when given a working cancellation signal. The gap is the one-shot PR adapter's wiring, not an absence of process management throughout Ferrite. See `crates/ferrite-core/src/server.rs:1116-1129` and `crates/ferrite-reviewr/src/proc.rs:106-145`.

## Verification gaps and proposed runtime checks

No runtime reproduction is claimed. The parent owns live UI access. An isolated headless store with a fake forge should settle these checks without modifying the user's repository or real PRs.

1. Open a PR with pending checks, add a comment in the fixture, run Refresh PR status, and capture whether detail changes. This validates 001 and distinguishes the status badge from the native detail.
2. Force a load failure after one success, press the advertised r, and test the menu refresh. Capture the visible content and actions. This validates 002 and 003.
3. Use two guide comments and dated discussion in canonical backend order. Capture Guide and Timeline. This validates 004 and 005.
4. Load resolved/outdated findings with replies, code snippets and each truncation flag. Verify visible content and completeness warnings. This validates 006 and 007.
5. Open a prepared branch in a fresh thread with no instrumented edits. Inspect whether Create PR exists. This validates 020 without performing a forge write.
6. Use delayed responses A/B with different PR numbers. Deliver B first. Verify A cannot overwrite B or retire its tab. This is required to reproduce the inferred race in 015.
7. Use a fake gh process that blocks. Close/supersede the request and inspect its process tree and request count. This validates the lifecycle impact in 016; no CPU or latency measurements were made.
8. Inspect generated prompts with fork/upstream, enterprise and non-GitHub fixtures. Do not run destructive commands. Confirm target identity before assessing the inferred wrong-target risk in 014.
9. Exercise action queuing while an agent is busy. Verify whether anything outside the inspected panel unexpectedly provides pending/result state. This checks the installed implementation against source finding 012.

The native mutation parity gaps are source-defined absence in this panel, not proof that an agent cannot perform the same operation through chat. The browser and separate Review slot provide alternative access but do not repair the panel's missing data or false refresh behavior.

No application tests or typecheck were run because this task changes only these audit artifacts. Report schema, unique IDs, evidence link formatting and required fields were checked locally after writing.

## Remediation order

First fix creation visibility, detail refresh and recovery. Restore data already in WirePr, then correct guide/timeline ordering and show truncation/sync state. Next add target-safe operation lifecycle and cancellation. Finally choose the native review, metadata-editing and structured-guide capabilities to adopt. This order repairs misleading existing behavior before expanding the product.
