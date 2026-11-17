# Subagent placement: a main conversation, not a compact sidebar transcript

The earlier audit recorded T3's full child-conversation route but did not carry that distinction into its repair recommendation. Unit 12 was titled “Preserve Agent capabilities in the ordinary panel route,” which assumed the compact right panel should remain the primary destination. That recommendation was incomplete.

## What the source and live observations establish

In the native T3 run, opening Docs Audit, Live Output Check, Sol Output Probe and the Sonnet child navigated to the child's own conversation route. The child occupied the main conversation area, showed its transcript and provider/status footer, and offered parent navigation. This is distinct from T3 Side chat, which is a right-panel surface. See the [native observations](../evidence/t3-live/observations.md) and [installed-source correspondence](t3-live-source-correspondence.md#child-pages-and-approvals-have-different-owners).

The exact installed source navigates to `/$environmentId/$threadId` in [useThreadLifecycle.tsx:494](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useThreadLifecycle.tsx#L494), then renders `ChatView` in the main `SidebarInset` at [ThreadRouteView.tsx:218](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadRouteView.tsx#L218). `SidebarInset` is the main content container's component name, not a claim that the child transcript is rendered inside the navigation sidebar.

Ferrite's ordinary agent-row click explicitly calls `open_agent_drill`; that builds a compact transcript and upserts `SurfaceId::Agent` into the right Panel. The source documents this routing directly in [agent_pane.rs:67](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/agent_pane.rs:67) and performs the panel insertion at [agent_pane.rs:110](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/agent_pane.rs:110). The [runtime reproduction](agent-live-correction.md) demonstrates the resulting compact Agent view and its clipping, nested navigation and reopen failures.

Ferrite already has full-Pane Subject navigation and history behavior. Therefore this does not inherently require making each provider child a new top-level Ferrite Thread or duplicating provider integration. The missing product decision is to use a complete child-conversation destination as the primary route. [Existing Subject capabilities](subagents-background.md).

## Corrected repair direction

The primary agent action should open the selected child in the main conversation area, at normal reading width. Show child identity, current status, provider/model, history coverage, immediate parent and root navigation. Preserve the parent's draft and reading position. Opening an agent must retain the identity of that agent throughout history loading, live output, nested-agent navigation and return.

A compact side-by-side inspector can remain an explicit secondary action if useful. Widening or maximizing the existing panel alone does not satisfy the primary navigation requirement. Provider child views can remain read-only where the provider owns execution; full conversation placement does not imply an independently editable composer.

Acceptance: open a child through the actual parent transcript action; verify that its body occupies the main conversation surface and identifies the child; open its nested child; navigate to the immediate parent and then root; confirm the root draft and scroll position survive; revisit the child and verify retained history and current output. Repeat for Sol and Sonnet, including waiting, completed and unavailable-history states. Keep any optional inspector's close/reopen behavior separately covered.

## Sidebar terminology

The right workspace panel and left thread navigation are different surfaces. This correction concerns the right-panel confinement of the child transcript. It does not claim T3 lists provider-native children as independent top-level rows in the left sidebar. T3's source treats provider children as related conversations reached through agent and lineage controls; ordinary-list visibility is a separate contract. A proposed expandable left-sidebar agent tree must be described as its own product requirement, not inferred from the existence of full child routes.

In the installed revision, [the lineage predicate](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/contracts/src/orchestrationV2.ts#L163) excludes provider children from ordinary rows, and [sidebar selection](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.logic.ts#L580) resolves an open child to its nearest listed ancestor. [Lineage navigation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L605) supplies the related-child destination.

This clarifies the placement and acceptance criteria underlying `subagents-background-02`, `subagents-background-04` and the existing Agent runtime findings. It does not invent an additional reproduced failure or change the 176-record historical tally. Application code remains unchanged. The earlier Sonnet artifact review predates this addendum.

A resumed source specialist checked the addendum and revised Unit 12 against the exact installed revision and Ferrite's opener. It found no factual correction needed and completed without starting any UI, model or test runtime. Earlier final-run snapshot hashes remain historical snapshots taken before this follow-up.
