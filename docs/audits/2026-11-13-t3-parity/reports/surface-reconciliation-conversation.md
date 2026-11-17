# Conversation surface reconciliation

The user wants the audit to identify complete conversation surfaces and correct destinations without asking them to point out each missing panel or misplaced child view. This map follows installed T3's rendered component branches and compares their actual assembly with Ferrite at `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.

throughput checkpoint: n/a, read-only investigation.

## Overview

There are 32 semantic surface records below. They are grouped from the actual ChatView, MessagesTimeline, and ChatComposer return branches, rather than a generic feature checklist. All T3 source links use installed `1264c206aee0dce805d6279809118bc091f29d2f`. The `74573fc29b901bafbf8f1dbb2eb29606da1fa448` checkout helped locate file names only. Earlier source reports using that newer revision remain qualified comparison evidence.

Two confirmed audit omissions are already corrected by the parent. [Subagent placement](subagent-placement-correction.md) requires a main child conversation as the primary destination. [Thread details](thread-details-card-correction.md) requires an independently accessible card beside the conversation, rather than an Overview tab inside the tool panel. This report does not double-count those corrections or change the finding tally.

Other gaps below concern omitted composition, focus, lifecycle, and conditional-branch coverage. They are source-backed audit omissions, not newly reproduced Ferrite failures. A named component in source or an inspected menu is not a passed user journey. Native evidence remains in the [parent ledger](../evidence/t3-live/observations.md) and [live source correspondence](t3-live-source-correspondence.md).

## The rendered hierarchy establishes the destination

```text
ThreadRouteView / main SidebarInset
  ChatView, root or provider-child conversation
    nonembedded ChatHeader + independent layout controls
    ThreadFindCanvas / ChatCanvas
      ThreadFind overlay + WorkspaceFileDropOverlay
      ChatBannerLane, provider status and thread errors
      ThreadTimeline / MessagesTimeline
        parent context header + earlier history / errors
        virtual rows, work groups, tools, agents, artifacts
        inline pending approvals + present-state / guest / queue footer
        AssistantSelectionToolbar + timeline minimap
      ScrollToEndButton, hidden while thread Find is active
      composer overlay
        optional draft hero headline
        ChildThreadApprovals stack
        ProviderSubagentBar
          optional Antigravity NativeChildWorkDialog
        or independently editable ChatComposer
          banners / question drawer / stash badge
          command or stash menu / attachments / context chips / editor
          validation / voice / adaptive controls / primary action
        optional branch / execution-context strip
      nonembedded preview sync / ThreadPreviewMiniPlayer
      nonembedded BranchRestoreConfirmHost / ThreadDetailsPanel / PR-thread dialog
    nonembedded ChatTerminalDrawers
    nonembedded ChatRightPanel, inline or sheet workspace
      SideChatPane -> embedded ChatView, not a provider-child main route
    RevertConfirmHost / nonembedded LinkPullRequestDialogHost
    ThreadVisitWatermark, nonvisual
    ExpandedImageDialogHost, thread-scoped
```

The mount sites are [ChatView.tsx:3270](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3270), [ChatView.tsx:3336](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3336), [ChatView.tsx:3380](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3380), [ChatView.tsx:3489](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3489), [ChatView.tsx:3832](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3832), [ChatView.tsx:3915](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3915), and [ChatView.tsx:3995](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3995). The complete row-kind dispatch is [MessagesTimeline.tsx:1573](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/MessagesTimeline.tsx#L1573). It includes work, work-live, work-toggle, thinking, turn-fold, attempt-fold, compaction, user/assistant messages, assistant metadata, plan, HTML, viewed images, MCP app, working, worktree setup, and event rows. Event rows separately dispatch subagent groups, secret requests, and link requests. Nonvisual contexts, data hooks, boundary providers, styling wrappers, and icons are not counted as separate product surfaces.

## What the user should not need to discover next

- Child full conversation, inline group, Lineage, optional compact inspector, Side chat, and Antigravity-native work dialog need different entry and destination assertions. One agent screenshot cannot cover them.
- Find, minimap turn navigation, earlier-history loading, and a fold's lazy work history have different ownership. A successful latest-answer search does not verify all of them. Installed thread Find excludes command-output rows.
- Active approval rows, related-child approval stack, question drawer, secret form, and machine-link wizard are different request presentations. A generic Decision implementation cannot certify their input ownership or failure recovery.
- Top provider/thread errors, composer sending notices, timeline present-state, handoffs, guest proposals, and queued own input have different placement and lifetimes. A working toast or Background tab is not a substitute.
- Citation selection, per-quote comments, structured context details, draft-file preview, and expanded media need separate opener, Escape, focus-return, route-change, and content-retention checks.
- Plans, rendered HTML, viewed tool images, and MCP app inline/fullscreen viewers are conditional transcript children. Their source existence was not a complete installed-surface comparison in the prior audit. Optional adoption and unexecuted branches must remain explicit.
- Responsive mode is part of the surface contract. Embedded side-chat suppression, provider-child read-only footer, maximized-chat inertness, draft hero docking, narrow details popover, collapsed composer, and mobile question actions cannot be inferred from one ordinary desktop root screenshot.

These gaps are assigned to concrete records below. The [machine-readable map](surface-reconciliation-conversation.json) also retains exact source fingerprints and the lexical JSX dispatch inventory so another reviewer can audit the boundaries.

## Surface-by-surface contracts

### C01. Main conversation route

Read and operate the selected root conversation. Main SidebarInset content, containing ChatView, not either sidebar. Triggered by select root thread or route directly.

Coexistence. Header, timeline, composer, independent details card and workspace siblings. Responsive contract. Workspace maximization gives chat w-0 and inert; embedded mode has different chrome. Ownership. Scoped environment and thread; draft routes resolve separately.

Focus. Header menus, composer, Find, workspace have separate focus areas. Motion. Panel or draft movement has explicit gates; route change is not proof of transition. Lifetime. Thread mount and remembered ready timeline are keyed; displayed and requested data can differ while loading.

Installed sources. [ThreadRouteView.tsx:218](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadRouteView.tsx#L218), [ChatView.tsx:3270](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3270), [ThreadTimeline.tsx:14](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadTimeline.tsx#L14).

Ferrite counterpart. Ordinary Pane and TranscriptView. [pane.rs:881](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L881), [transcript.rs:2912](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2912). Wrong substitute. A right-panel Agent inspector or Overview tab is not the main conversation.

Prior coverage. Main transcript covered; composition previously scattered. Audit omission or remaining boundary. No additional root-feature defect asserted. Existing reports. [subagent-placement-correction.md](subagent-placement-correction.md), [transcript.md](transcript.md). Native boundary. Native roots exercised.

### C02. Provider child full conversation

Inspect a provider-owned child with its own complete transcript. Normal main conversation route, reached from agent/lineage actions. Triggered by onOpenRelatedThread navigates to /$environmentId/$threadId; lineage does likewise unless side chat.

Coexistence. Parent breadcrumb, transcript, footer and optional workspace; parent draft stays with parent. Responsive contract. Normal reading lane; a narrow optional inspector is a separate product choice. Ownership. Selected child projection; provider owns execution and may forbid independent prompting.

Focus. Open parent returns through scoped navigation; child footer is not an input. Motion. No main-child entrance animation is proven by the route source. Lifetime. Child route loads its own detail, including omitted idle children.

Installed sources. [useThreadLifecycle.tsx:463](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useThreadLifecycle.tsx#L463), [ThreadRelationshipsControl.tsx:604](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L604), [ThreadRouteView.tsx:218](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadRouteView.tsx#L218), [ChatView.tsx:3543](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3543), [ProviderSubagentBar.tsx:14](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14).

Ferrite counterpart. Full-Pane Subject selection exists, but ordinary row action opens a compact Agent panel. [agent_pane.rs:67](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L67), [subagents.rs:376](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L376), [side_chat.rs:477](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L477). Wrong substitute. Widening/maximizing Agent panel alone does not meet the primary main-conversation placement contract.

Prior coverage. Live route was recorded but original Unit 12 assumed compact Panel as primary. Audit omission or remaining boundary. Confirmed recommendation/placement omission, corrected by dedicated addendum. Existing reports. [subagent-placement-correction.md](subagent-placement-correction.md), [agent-live-correction.md](agent-live-correction.md), [subagents-background.md](subagents-background.md). Native boundary. Sol and Sonnet child routes and live stdout verified; full nested lifecycle is not universally certified.

### C03. Conversation header and parent breadcrumb

Identify project, parent and selected thread; expose thread actions and Find entry. Header above canvas, suppressed for embedded side chats. Triggered by existing thread/draft and parent link; title activation or context menu.

Coexistence. Separate from panel toggles and details-card toggle in same title region. Responsive contract. Parent title collapses to fit measured breadcrumb width. Ownership. Selected thread/project; parent may belong to another environment.

Focus. Semantic title button/menu and rename input; breadcrumb can open parent. Motion. Hover/title transitions are CSS details, not panel motion. Lifetime. Mounted with route header; embedded ChatView omits this header.

Installed sources. [ChatHeader.tsx:247](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatHeader.tsx#L247), [ChatView.tsx:3270](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3270).

Ferrite counterpart. Pane title/Subject navigation and menus. [pane.rs:881](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L881), [subagents.rs:376](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L376). Wrong substitute. Generic panel title cannot identify full conversation ancestry.

Prior coverage. Thread navigation covered; parent/title placement was not a unified contract. Audit omission or remaining boundary. Require breadcrumb and menu destination checks in child and root modes. Existing reports. [projects-navigation.md](projects-navigation.md), [subagent-placement-correction.md](subagent-placement-correction.md). Native boundary. Parent breadcrumb and Open parent verified; every title/rename variant not certified.

### C04. Virtualized transcript and earlier-history control

Render live answers and retrieve prior turns without losing reading position. Scrollable main lane; earlier-history/error control is its list header. Triggered by projection deltas, route switch, Load earlier, or citation/Find history demand.

Coexistence. Composer overlay reserves content inset; header and footer can coexist with long history. Responsive contract. Min-width and lane inset come from ChatCanvas measurements; long content must wrap. Ownership. Per-thread live turn items; remembered ready transcript excludes ephemeral previews.

Focus. Focusable earlier-history button; history error/retry are visible rather than silent blank content. Motion. Maintain-visible-content positioning and scroll adjustments are separate from appearance animation. Lifetime. Ready timeline cache is keyed; earlier pages and work-history loads are independent.

Installed sources. [ThreadTimeline.tsx:14](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadTimeline.tsx#L14), [MessagesTimeline.tsx:1392](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/MessagesTimeline.tsx#L1392), [MessagesTimeline.tsx:1481](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/MessagesTimeline.tsx#L1481), [ChatCanvas.tsx:18](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatCanvas.tsx#L18).

Ferrite counterpart. TranscriptView list with retained rendering window and scroll anchors. [transcript.rs:2912](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2912), [scroll.rs:110](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L110). Wrong substitute. A current rendered window is not proof that earlier persisted history can be loaded.

Prior coverage. Source and controlled scroll coverage exist; historical paging runtime remains bounded. Audit omission or remaining boundary. No newly reproduced defect; retain paging/loading/error contract separately. Existing reports. [transcript.md](transcript.md), [scroll-quality.md](scroll-quality.md), [scroll-runtime-correction.md](scroll-runtime-correction.md). Native boundary. Rich output and selected navigation verified; all historical paging states not executed.

### C05. Find in thread overlay

Search conversation messages with counts and exact occurrence navigation. Absolute top-right canvas overlay, not a workspace tab or browser Find. Triggered by thread Find command or header action, when server capability permits.

Coexistence. Reserves details-card top inset; parent PR/tool panel can remain open. Responsive contract. Width is capped by details-card width and available chat width. Ownership. Provider owns query/count state below ChatView; server filters message/plan/input candidates.

Focus. Open/refire focuses and selects input; Enter/Shift-Enter navigate; close callback focuses composer. Motion. 150ms opacity entrance with reduced-motion opt-out. Lifetime. One scoped Find store; content changes refresh results; history reveal may materialize unloaded rows.

Installed sources. [ChatView.tsx:3336](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3336), [ThreadFindProvider.tsx:10](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadFindProvider.tsx#L10), [ThreadFindBar.tsx:26](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadFindBar.tsx#L26), [ThreadFind.ts:311](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/orchestration-v2/ThreadFind.ts#L311), [useThreadFindNavigation.ts:174](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useThreadFindNavigation.ts#L174).

Ferrite counterpart. Transcript Find bar searches rendered logical-text projection. [find.rs:40](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/find.rs#L40), [transcript.rs:2912](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2912). Wrong substitute. Browser Find, file Find, or a generic panel search is not conversation Find.

Prior coverage. Prose Find and Ferrite folded exclusion covered; prior broad T3 tool-search assumption corrected. Audit omission or remaining boundary. Must preserve exact search scope; installed T3 excludes command_execution regardless of fold. Existing reports. [transcript.md](transcript.md), [scroll-runtime-correction.md](scroll-runtime-correction.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md). Native boundary. AMBER counts/Enter verified; command-only token returned 0/0; not data loss.

### C06. Timeline minimap and previous/next-turn navigation

Show turn positions and move directly among prompts. Beside timeline list, not the left thread sidebar. Triggered by hover/focus navigation buttons or strip activation.

Coexistence. Find and panel state are independent; selected turn reveals in same conversation. Responsive contract. Gutter tracks actual list viewport and content width. Ownership. Derived from current rows, visible range and prompt row indices.

Focus. Buttons have Previous turn/Next turn labels; no-op/disabled state depends on current range/end. Motion. Requested turn scroll uses reduced-motion gate; strip/button hover opacity is separate. Lifetime. Current index refreshes from layout and scroll metrics; not a saved chapter system.

Installed sources. [MessagesTimeline.tsx:1455](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/MessagesTimeline.tsx#L1455), [TimelineMinimap.tsx:266](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelineMinimap.tsx#L266).

Ferrite counterpart. Ferrite has a minimap rail, prompt/failure/Decision/pass ticks, viewport band, and click/rail jumps. Equivalent previous/next-turn buttons and full keyboard/focus behavior were not established. [transcript.rs:2757](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2757), [transcript.rs:2912](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2912), [scroll.rs:110](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L110). Wrong substitute. Pin as chapter or ordinary scroll is not equivalent to turn overview/navigation.

Prior coverage. Motion report already covers minimap snap navigation; native T3 verified one Next turn. Previous reconciliation lacked explicit comparison of the existing rail and adjacent-turn controls. Audit omission or remaining boundary. Detailed destination/focus acceptance gap, not an absent minimap or new Ferrite runtime failure. Existing reports. [motion.md](motion.md), [transcript.md](transcript.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md). Native boundary. Next turn reached quote/comment/image turn with PR retained; full minimap interaction untested.

### C07. Work groups, live row and historical folds

Present commands/reasoning and elapsed activity without consuming the entire transcript. Inline work-log rows and bounded expandable history inside timeline. Triggered by running work, disclosure, turn/attempt folds, work-history demand.

Coexistence. Final prose, pending approval row, working footer and group detail have distinct positions. Responsive contract. Expanded virtual list caps at min(18rem,50dvh) plus opened detail height. Ownership. Disclosure and expanded-entry state belong to group; historic turn data can load lazily.

Focus. Header/row aria-expanded and keyboard disclosure; nested scrolling must remain reachable. Motion. Chevron transitions, activity shimmer and append-follow exist; grouping uses selected nonanimated panels. Lifetime. Expansion retains entry IDs within group view; loading skeleton and history errors are independent.

Installed sources. [MessagesTimeline.tsx:1573](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/MessagesTimeline.tsx#L1573), [TimelineWorkGroupRows.tsx:248](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelineWorkGroupRows.tsx#L248), [TimelineWorkEntryRow.tsx:223](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelineWorkEntryRow.tsx#L223).

Ferrite counterpart. Work rows, grouped calls, turn folds and thinking tail. [transcript.rs:1654](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1654), [transcript.rs:1712](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1712). Wrong substitute. A tool name or final summary alone does not show current output or full historical details.

Prior coverage. Core source and controlled disclosure covered. Audit omission or remaining boundary. Explicit row dispatcher now prevents treating every history form as one generic tool block. Existing reports. [transcript.md](transcript.md), [scroll-quality.md](scroll-quality.md), [agent-live-correction.md](agent-live-correction.md). Native boundary. Several command disclosures and live output verified; all fold/history states untested.

### C08. Tool item inspector and live output

Inspect actual arguments, stdout, diff, images and structured tool results. Inline WorkLogDetails, not a global tool inspector window. Triggered by expand individual tool entry or fetch retained output.

Coexistence. Inside bounded group; open diff can hand off to full workspace while transcript stays. Responsive contract. Pre/code content has its own overflow; environment capability gates output subscription. Ownership. Thread/item identity keys output; command deltas and final persisted result are distinct sources.

Focus. Expanded row uses keyboard disclosure; image/diff buttons have separate actions. Motion. Chevron/row interactions do not imply streaming transition; stdout subscription is data flow. Lifetime. Inspector mounts on expanded entry; live panel follows until user scrolls away and settles to final output.

Installed sources. [TimelineWorkEntryRow.tsx:223](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelineWorkEntryRow.tsx#L223), [V2ItemInspector.tsx:231](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/V2ItemInspector.tsx#L231), [CommandOutputPanel.tsx:26](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/CommandOutputPanel.tsx#L26).

Ferrite counterpart. Expanded tool call rows and native preview/diff destinations. [transcript.rs:1712](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1712), [rich.rs:535](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L535). Wrong substitute. Background panel with No output yet is wrong when the child transcript contains known output.

Prior coverage. Actual live streams covered; some tool-result branches remained source-only. Audit omission or remaining boundary. Keep item inspection distinct from agent full conversation and background roster. Existing reports. [agent-live-correction.md](agent-live-correction.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md), [panel-tools.md](panel-tools.md). Native boundary. Sol parent/child and Sonnet child stdout verified before completion.

### C09. Inline agent group, agent links and hover information

Show related launched agents, aggregate progress and links into each child. Inline work-log group inside parent transcript. Triggered by subagent item; group only when row has more than one agent.

Coexistence. Lineage card is an additional navigation view, not replacement for these rows. Responsive contract. Group has bounded avatars/count and truncated text with hover details. Ownership. Runtime projected subagent identity and status, not independent user thread rows.

Focus. Collapsible group and child link are distinct targets; hover card does not become editable chat. Motion. Opacity/chevron CSS; group panel explicitly animate=false. Lifetime. Open child loads own route; group expansion belongs to row state.

Installed sources. [TimelineV2EventRow.tsx:19](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelineV2EventRow.tsx#L19), [V2LifecycleRow.tsx:421](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/V2LifecycleRow.tsx#L421).

Ferrite counterpart. Grouped agent rows and progress exist; opener currently routes to compact Panel. [transcript.rs:1654](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1654), [agent_pane.rs:67](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L67). Wrong substitute. Left-sidebar tree or background card does not satisfy inline launch context plus correct main route.

Prior coverage. Group mechanics covered; initial repair direction missed destination size/placement. Audit omission or remaining boundary. Link group must be reconciled with corrected main-child action, not merely retain clickable pills. Existing reports. [subagents-background.md](subagents-background.md), [subagent-placement-correction.md](subagent-placement-correction.md), [agent-live-correction.md](agent-live-correction.md). Native boundary. Real children and prior-agent movement verified; all hover states not native certified.

### C10. Independent Thread details card

Keep compact execution and thread context available beside conversation. Absolute upper-right aside, or dedicated anchored popover when space is insufficient. Triggered by independent toggle/shortcut; per-thread presentation state.

Coexistence. Separate from full workspace; card layout avoids Find and preview/composer footprints. Responsive contract. 280px source card with density full/compact/essential; placement fallback is measured, not a freely dragged window. Ownership. Scoped environment/thread and independent inline/popover visibility.

Focus. Popover requests details focus; Find interactions do not force it closed; Escape returns to toggle in native recheck. Motion. Top-inset transition and popup lifecycle; no arbitrary drag/resize/minimize contract established. Lifetime. Popup stays in tree across mode changes to close cleanly; details state is not workspace active-tab state.

Installed sources. [ChatView.tsx:3832](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3832), [ThreadDetailsCard.tsx:131](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsCard.tsx#L131), [threadDetailsCardLayout.ts:17](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/threadDetailsCardLayout.ts#L17), [ChatCanvas.tsx:18](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatCanvas.tsx#L18).

Ferrite counterpart. Overview has related information inside the tool workspace. [sections.rs:25](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/sections.rs#L25), [panel.rs:624](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L624). Wrong substitute. Overview tab, narrower forced panel width, or copied card styling alone cannot coexist equivalently with a loaded tool.

Prior coverage. Previously named toggle/lineage but independent presentation omitted. Audit omission or remaining boundary. Confirmed quality-correction-005, already added by parent; do not double-count. Existing reports. [thread-details-card-correction.md](thread-details-card-correction.md), [panel-tools.md](panel-tools.md). Native boundary. Dedicated recheck verified toggle, shortcut, focus, Escape and one fallback. Subsequent parent-owned native evidence verified Thread details beside loaded README with an intact draft; all-tool and narrow-screen variants remain open. See ../evidence/surface-reconciliation/observations.md.

### C11. Lineage and Side chats sections inside details

Show parent/child/fork/transfer relations and related side conversations. Conditional sections inside full-density details card; not entire details host. Triggered by existing related/active/previous/remote rows or side chats; visibility customization.

Coexistence. Workspace/Git/usage/context sections share card; inline agent group remains in transcript. Responsive contract. Only full density mounts these sections; pagination limits visible lineage rows. Ownership. Graph distinguishes subagent, fork, transfer, sideChat and linked-environment relations.

Focus. Ordinary children navigate full route; side-chat predicate opens side tab; tooltip carries details. Motion. Group disclosure state; no new native transition guarantee asserted. Lifetime. Active/previous grouping is status-derived and detail may load after navigation.

Installed sources. [ThreadDetailsPanel.tsx:151](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsPanel.tsx#L151), [ThreadRelationshipsControl.tsx:586](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L586), [ThreadRelationshipsControl.tsx:604](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L604).

Ferrite counterpart. Subject ancestry data exists; main drill and compact rendering lose some navigation. [subagents.rs:376](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L376), [sections.rs:25](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/sections.rs#L25). Wrong substitute. A flat named-agent list cannot replace immediate-parent/root ancestry or distinguish side-chat destination.

Prior coverage. Original source coverage exists; card density/coexistence integration was omitted. Audit omission or remaining boundary. Correct placement and status scope jointly; do not claim provider children are ordinary left-sidebar rows. Existing reports. [subagents-background.md](subagents-background.md), [subagent-placement-correction.md](subagent-placement-correction.md), [thread-details-card-correction.md](thread-details-card-correction.md). Native boundary. Native parent/child label disagreement observed; not child loss.

### C12. Antigravity-native Subagent work dialog

Read provider-native child records that are not independently messageable. Modal dialog launched from provider-child footer only for selected antigravity driver. Triggered by subagent work button; query starts only while open.

Coexistence. Overlays child conversation; never substitutes for Sol/Sonnet normal route. Responsive contract. max-w-2xl popup; footer label can be screen-reader-only at narrow composer width. Ownership. nativeChildWork query keyed by environment/thread; refresh, loading/error/empty/truncation states.

Focus. Shared dialog primitive controls focus/Close; read-only child steps, no messaging input. Motion. Shared dialog 200ms source styles; no native timing measurement. Lifetime. Local open state; content/query unmounts when closed.

Installed sources. [ChatView.tsx:3543](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3543), [NativeChildWorkDialog.tsx:30](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/NativeChildWorkDialog.tsx#L30), [dialog.tsx:18](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/dialog.tsx#L18), [dialog-styles.ts:1](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/dialog-styles.ts#L1).

Ferrite counterpart. Provider-scoped history/coverage messaging exists; no exact Antigravity dialog counterpart established. [subagents.rs:376](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L376), [side_chat.rs:477](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L477). Wrong substitute. Compact Agent panel is not justification to replace normal child routes with a read-only modal for all providers.

Prior coverage. Already covered and provider-qualified in source; not a newly discovered missing feature. Audit omission or remaining boundary. Audit still needs branch-specific execution if this optional provider is adopted. Existing reports. [subagents-background.md](subagents-background.md). Native boundary. Antigravity dialog was not executed in native audit.

### C13. Draft hero and ordinary composer host

Compose new conversation or next message while retaining transcript reading space. Centered headline/composer for empty draft; bottom overlay for active thread. Triggered by draft hero predicate, project selection, transition toward first send.

Coexistence. Banners detach below hero but join active composer; branch/environment strip below host. Responsive contract. Short-height media rule pins hero toward bottom; mobile composer can collapse to prompt summary. Ownership. Draft/session binding separate from existing thread and queued-edit target.

Focus. Typed editor owns IME; focus restoration and mobile expansion are distinct from panel focus. Motion. Draft hero/view-transition names and composer background transitions exist; no native interpolation measured here. Lifetime. Composer may stay mounted across threads; provider-child branch unmounts it entirely.

Installed sources. [ChatView.tsx:3508](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3508), [ChatView.tsx:3489](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3489), [ChatComposer.tsx:3212](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3212), [ChatComposer.tsx:3318](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3318), [ChatView.tsx:3733](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3733).

Ferrite counterpart. Native composer region and draft binding. [composer.rs:456](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L456), [pane.rs:881](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L881). Wrong substitute. An always-visible generic input or child footer cannot stand in for all draft/running/question states.

Prior coverage. Draft persistence and input mechanics covered; centered/overlay/mobile assembly not systematically retained. Audit omission or remaining boundary. Presentation/lifetime coverage gap, not newly reproduced loss. Existing reports. [composer.md](composer.md), [draft-lifecycle-runtime-correction.md](draft-lifecycle-runtime-correction.md). Native boundary. Existing-draft and combined content flows observed; mobile/first-send animation unexecuted.

### C14. Composer model, effort, permissions and primary actions

Show provider settings and the actual send/steer/queue/stop/resume action. Footer or resting controls host within composer, plus optional context strip. Triggered by measured room, focus/resting state, active turn, pending question, nonempty draft.

Coexistence. Does not replace question drawer or inline approval details; control relocation must preserve identity. Responsive contract. Controls can hide blocks/use icons; mobile pending answers move primary actions inside editor area. Ownership. Selected model and effective runtime policy; Stop key belongs to thread, not reused composer instance.

Focus. Model picker and permission menus have explicit open state; pointer-focus preservation varies by viewport. Motion. Source hover/background transitions; stop pending state is functional feedback. Lifetime. Footer action keyed by environment/thread; resumed route cannot reuse old stop state.

Installed sources. [ChatComposer.tsx:2404](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L2404), [ChatComposer.tsx:3591](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3591), [ChatComposer.tsx:3318](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3318).

Ferrite counterpart. Model/effort/permission chips, native primary actions. [composer.rs:456](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L456), [decision.rs:678](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/decision.rs#L678). Wrong substitute. Showing one Stop icon does not certify queue timing, question submission or effective sandbox truth.

Prior coverage. Source and bounded actual provider setting/Stop checks covered. Audit omission or remaining boundary. Require relocation/focus and action-state matrix rather than a single screenshot. Existing reports. [composer.md](composer.md), [providers-settings.md](providers-settings.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md). Native boundary. Sol Stop, Sonnet Stop/Resume verified; Sol Resume and held queue not executed.

### C15. Command, entity and prompt-stash overlays

Select commands/context or restore independent stashed drafts. Anchored menu above composer body. Triggered by typed trigger or stash badge; stash only when command menu is closed.

Coexistence. Menu and stash are mutually selected; ordinary draft remains separate from chosen context. Responsive contract. Anchor and popup width govern narrow layout; no desktop screenshot establishes mobile behavior. Ownership. Draft context records and selected provider catalogs; stash queue has independent entries.

Focus. Editor active suggestion ID and highlighted item coordinate keyboard navigation. Motion. Menu layer/popover motion comes from primitives; native motion unmeasured. Lifetime. Menus are ephemeral; stashes are draft records, not sent queue runs.

Installed sources. [ChatComposer.tsx:3393](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3393), [ChatComposer.tsx:3461](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3461), [popover.tsx:66](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/popover.tsx#L66).

Ferrite counterpart. Native command/file/skill menus; independent prompt stash not established. [composer.rs:456](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L456), [quote.rs:50](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L50). Wrong substitute. Queued message editing or prompt history is not a stash menu with independent draft/context entries.

Prior coverage. Feature differences covered; overlay collision/focus not fully exercised. Audit omission or remaining boundary. Retain mutual-exclusion and outside/Escape behavior as explicit acceptance. Existing reports. [composer.md](composer.md). Native boundary. Native command/entity/stash authoring variants not certified by the combined send run.

### C16. Assistant selection Cite toolbar

Capture a selected passage with source identity. Portal near selection, clamped inside browser window. Triggered by valid assistant selection and citation callback; keyboard focus shortcut.

Coexistence. Can sit above transcript while composer stays mounted; source selection is not whole-response Reply. Responsive contract. Window clamp prevents toolbar overflow; max selected length can disable citing. Ownership. Selected thread/message/range and source anchor.

Focus. Pointer down preserves selection; keyboard can focus toolbar; callback inserts into owning composer. Motion. No toolbar entrance interpolation was established from inspected code. Lifetime. Selection observer/portal disappears with selection or source lifecycle.

Installed sources. [MessagesTimeline.tsx:1392](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/MessagesTimeline.tsx#L1392), [AssistantSelectionToolbar.tsx:148](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L148).

Ferrite counterpart. Native floating Cite selection action creates Reply text. [reply.rs:158](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/reply.rs#L158), [quote.rs:50](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L50). Wrong substitute. General response context menu or quoted text alone does not preserve selected-source identity.

Prior coverage. Selection behavior covered; whole-response Reply is newer-only and remains qualified. Audit omission or remaining boundary. No new absence; keep precise insertion owner and selection/focus contract. Existing reports. [quote-attachments.md](quote-attachments.md), [transcript.md](transcript.md), [t3-installed-source-match.md](t3-installed-source-match.md). Native boundary. Native selection and Cite worked with markers.

### C17. Citation chip comment editor and sent source card

Author, reopen and cancel per-quote comments; return from sent quote to source. Anchored comment popover in draft; source link plus comment in sent user row. Triggered by cite insertion or comment edit action; sent card activation navigates.

Coexistence. Ordinary prompt and images coexist; comment draft differs from saved comment. Responsive contract. Popover and multiline editor bound to source; full comment whitespace preserved in sent card. Ownership. Citation schema stores environment/thread/message/text/context/comment; source navigation is explicit.

Focus. Textarea supports multiline, Save/Cancel and submit-and-send; focus return tied to chip/source. Motion. Shared popup transitions; no native animation timing claim. Lifetime. Uncommitted comment draft clears on close; source observation detects disappearance; sent citation remains.

Installed sources. [AssistantCitationChip.tsx:108](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/AssistantCitationChip.tsx#L108), [AssistantCitationCommentEditor.tsx:27](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L27), [assistantCitationNavigation.ts:79](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/assistantCitationNavigation.ts#L79).

Ferrite counterpart. Reply stores quote/text but UI has no per-quote authoring path and no navigable source identity. [quote.rs:50](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L50), [attachments.rs:551](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L551). Wrong substitute. Ordinary composer commentary is not an editable comment associated with a citation.

Prior coverage. Known comment/provenance gaps source-backed; actual combined T3 delivery verified. Audit omission or remaining boundary. Keep comment, quote disclosure and source navigation as separate contracts. Existing reports. [quote-attachments.md](quote-attachments.md), [attachment-runtime-correction.md](attachment-runtime-correction.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md). Native boundary. Save/reopen/cancel, combined send and source return verified; transient highlight not established.

### C18. Structured context chips and details popovers

Inspect terminal, review, preview annotation, thread, machine, image or file context without leaving draft. Inline chips in editor; some open top-side details popover, others preview media or navigate. Triggered by record kind decides renderer and action; unresolved record gets explicit fallback.

Coexistence. Review diff or annotation crop detail can coexist with composer and workspace. Responsive contract. Chip truncation is paired with detail presentation; popover width bounded by shared primitive. Ownership. Draft context records keep typed identities; unavailable record is not silently substituted.

Focus. Accessible Show details trigger; separate remove/action buttons and source-aware context callbacks. Motion. Shared popup transition and chip hover underline; source-only timing. Lifetime. Popover ephemeral; records persist with draft and bind outgoing attachment IDs.

Installed sources. [composerContextPresentation.tsx:364](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/composerContextPresentation.tsx#L364), [contextChipParts.tsx:64](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/contextChipParts.tsx#L64), [ChatComposer.tsx:3461](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3461).

Ferrite counterpart. Attachment shelf, review count, browser annotations and path tokens. [attachments.rs:551](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L551), [quote.rs:50](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L50). Wrong substitute. An 88px tile, count-only chip or raw filename is not complete inspectable structured context.

Prior coverage. Capabilities covered in slices; per-kind opener/destination assembly was not reconciled. Audit omission or remaining boundary. Important inspection/ownership audit gap; no blanket Ferrite absence asserted for every kind. Existing reports. [quote-attachments.md](quote-attachments.md), [composer.md](composer.md), [browser.md](browser.md). Native boundary. Preview annotation detail and actual provider receipt observed; all other record kinds not certified.

### C19. Draft attachment strip and file preview dialog

Show attachment readiness, thumbnail, retry/removal and full file preview. Media row/file list above editor; non-image preview is a modal dialog. Triggered by attach/paste/drop; media preview or filename activation.

Coexistence. Does not consume workspace active tab just to inspect unsent file. Responsive contract. Media absent when mobile collapsed; separate resting thumbnails; dialog width/height bounded. Ownership. Attachment draft target, upload/preparation state, nonpersisted warnings.

Focus. Named Preview/Remove/Retry controls; file dialog Close retains draft. Motion. Snapshot frame opacity/scale with reduced-motion classes; ordinary image row is not proven animated. Lifetime. Preview file ID is local; removing/closing affects preview/draft separately.

Installed sources. [ComposerAttachmentStrip.tsx:136](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L136), [ComposerAttachmentStrip.tsx:430](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L430), [ComposerAttachmentStrip.tsx:534](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L534).

Ferrite counterpart. Native attachment tiles and file/image preview destinations exist. [attachments.rs:551](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L551), [preview.rs:174](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L174). Wrong substitute. Path pasted into text or sent-file workspace tab is not unsent attachment readiness and inspection.

Prior coverage. Readiness and clipping source issues known; file chooser runtime limited. Audit omission or remaining boundary. Need classify preview route per draft type rather than call all previews equivalent. Existing reports. [composer.md](composer.md), [quote-attachments.md](quote-attachments.md), [attachment-runtime-correction.md](attachment-runtime-correction.md). Native boundary. Native T3 PNG attach/remove affordance/preview verified; Ferrite file intake not exercised by prior controlled correction.

### C20. Expanded image/video dialog

Inspect media from draft, history or tool output at usable size. Media modal above other composer/popover overlays. Triggered by onImageExpand or attachment/media chip preview.

Coexistence. Underlying draft and workspace retain state; nested context menu owns Escape before dialog. Responsive contract. 92vw/92vh media frame and zoom/pan; not mobile bottom-sheet variant. Ownership. Atom carries owning threadKey and preview; owned blob URL revoked when replaced.

Focus. Initial Close focus; explicit saved opener restoration after unmount; arrows navigate/pan with context-menu guard. Motion. Shared media dialog opacity/scale 200ms source styles; no measured runtime smoothness. Lifetime. Leaving route clears preview through thread-overlay registry; close clears atom.

Installed sources. [ChatView.tsx:3995](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3995), [ExpandedImageDialog.tsx:80](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ExpandedImageDialog.tsx#L80), [ExpandedImageDialog.tsx:292](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ExpandedImageDialog.tsx#L292), [dialog.tsx:18](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/dialog.tsx#L18), [dialog-styles.ts:1](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/dialog-styles.ts#L1).

Ferrite counterpart. Native Preview host handles images; complete equivalent media modal/focus contract not established. [preview.rs:174](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L174), [attachments.rs:551](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L551). Wrong substitute. Opening a generic reader or workspace image tab is not proof of equivalent modal nesting and focus return.

Prior coverage. PNG dialog visible and Escape worked; full image-dialog lifetime was not explicitly mapped. Audit omission or remaining boundary. Source-level focus and route-scoping gap in previous reconciliation. Existing reports. [quote-attachments.md](quote-attachments.md), [input-focus.md](input-focus.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md). Native boundary. T3 filename/100% preview and draft preservation verified; multi-image/video cases untested.

### C21. Queued messages and transactional edit target

Inspect pending future inputs and edit/promote/remove each by stable identity. Timeline end after current state and pending guest rows; editing uses composer. Triggered by queued run or uncommitted optimistic queued message.

Coexistence. Working state and queue both visible; unrelated main draft preserved by separate edit target. Responsive contract. Visible queue count limited with Show more; row actions wrap independently. Ownership. Run/message identity and separate queued-edit draft target.

Focus. Edit/Cancel/Save/Send now have explicit callbacks; saving makes composer host inert/aria-busy. Motion. No queue transition claim; source command success determines lifecycle. Lifetime. Optimistic rows disappear after committed; save clears only successful edit target.

Installed sources. [ChatView.tsx:3001](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3001), [ChatView.tsx:3091](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3091), [QueuedRunsControl.tsx:172](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/QueuedRunsControl.tsx#L172), [useQueuedRunEdit.ts:181](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useQueuedRunEdit.ts#L181), [ChatView.tsx:3489](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3489).

Ferrite counterpart. Queue cards/actions exist, but Edit cancels/takes back and merges draft. [composer.rs:791](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L791), [composer.rs:456](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L456). Wrong substitute. Prompt history, native steer queue or stash cannot substitute for transactional queued editing and timing intent.

Prior coverage. Known composer-04/05; controlled runtime plus real T3 one-item save. Audit omission or remaining boundary. Do not treat queue-card presence as cancellation/race/draft-isolation coverage. Existing reports. [composer.md](composer.md), [draft-lifecycle-runtime-correction.md](draft-lifecycle-runtime-correction.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md). Native boundary. Native T3 queue/edit/promote receipt passed; multi-item order/cancel/race not certified.

### C22. Ordinary pending approval inside timeline

Show exact proposed action and decision choices where it belongs in work history. Inline WorkLogRow; unmatched approvals appended at transcript list end. Triggered by pending approval projection not held by typing guard.

Coexistence. Composer remains for decline/redirect feedback; approval details and More options are separate. Responsive contract. Command text scrolls up to bounded height; actions wrap. Ownership. Request ID and live response capability; vanished provider cannot be approved.

Focus. Command is selectable; buttons and More options route the exact request; typing feedback has approval-specific meaning. Motion. Button pending feedback is distinct from generic loading; no native timing measurement. Lifetime. Pending row changes to settled history through provider events; response state prevents duplicate decisions.

Installed sources. [TimelinePendingApprovalRow.tsx:34](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelinePendingApprovalRow.tsx#L34), [MessagesTimeline.tsx:1392](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/MessagesTimeline.tsx#L1392), [ChatComposer.tsx:3265](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3265), [ComposerPendingApprovalActions.tsx:1](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L1).

Ferrite counterpart. Decision cards rendered in transcript tail; exact approval answer path shared. [decisions.rs:45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/decisions.rs#L45), [decision.rs:678](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/decision.rs#L678). Wrong substitute. A generic waiting badge or main-composer send must not approve an unrelated child.

Prior coverage. Bounded approvals covered; assembly with held typing/feedback and exact action not fully exercised. Audit omission or remaining boundary. Action ownership and coexistence need explicit matrix, not new blanket missing approval claim. Existing reports. [providers-settings.md](providers-settings.md), [forms-runtime-correction.md](forms-runtime-correction.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md). Native boundary. Real Sonnet separate command/edit/test approvals observed; final model claim of no approvals rejected.

### C23. Related-child approval stack above composer

Answer exact child-thread approvals without losing owning root context. Separate scrollable stack immediately above composer shell. Triggered by childThreadApprovalsAtom returns related approvals.

Coexistence. Distinct from active thread pending approval row and child read-only footer. Responsive contract. max-h-80 scroll stack; per-card command max-h-40. Ownership. Environment/thread/request triple keys cards; command submits entry.thread.threadId.

Focus. Per-card button disabled on missing operation scope or nonlive capability; card-level error is visible. Motion. No stack entrance animation established. Lifetime. Cards follow pending atom; local responding/error state keyed by exact request triple.

Installed sources. [ChatView.tsx:3489](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3489), [ChildThreadApprovals.tsx:15](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChildThreadApprovals.tsx#L15).

Ferrite counterpart. Core pending decisions retain Subject; compact drill omits Decision strip. [decisions.rs:45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/decisions.rs#L45), [subagents.rs:376](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L376), [side_chat.rs:477](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L477). Wrong substitute. Generic Open thread selecting first pending child is not an exact named-child request route.

Prior coverage. Child decision gaps known; separate T3 stack was not prominent in prior inventory. Audit omission or remaining boundary. Assembly/ownership omission; not proof that native Sonnet observed this exact component rather than parent-owned request row. Existing reports. [subagents-background.md](subagents-background.md), [forms-runtime-correction.md](forms-runtime-correction.md). Native boundary. Sonnet child command appeared in parent; source component reachability not uniquely established by that screenshot.

### C24. Pending question drawer and answer mode

Present questions, choice options and custom answer with request-scoped submission. Attached drawer above editor; editor becomes custom-answer field, primary actions become question actions. Triggered by pending user-input request; multiquestion progress controls current question.

Coexistence. Open question suppresses unrelated attention banners, retaining machine problems; attachments capability-gated. Responsive contract. Collapsible drawer and mobile inline action relocation; option text wraps. Ownership. Answers/questionIndex belong to runtime request, not ordinary draft text.

Focus. Digit shortcuts ignore editor contexts; single-choice can auto-advance; IME guard and required answer state matter. Motion. Collapsible and option-state CSS exist; no native multiquestion timing guarantee. Lifetime. Auto-advance timer cancels on request change/disable/unmount; ordinary draft must survive request lifecycle.

Installed sources. [ChatComposer.tsx:3265](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3265), [ComposerPendingUserInputPanel.tsx:130](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ComposerPendingUserInputPanel.tsx#L130), [ChatComposer.tsx:3461](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3461), [ChatComposer.tsx:3591](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3591).

Ferrite counterpart. Native request forms/choices exist; controlled field clicks routed to composer in bounded replay. [decision.rs:678](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/decision.rs#L678), [decisions.rs:45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/decisions.rs#L45). Wrong substitute. An ordinary composer or approval card is not a form with field validation and custom-answer ownership.

Prior coverage. Known form/focus uncertainty remains; one T3 single choice passed. Audit omission or remaining boundary. Need question/composer arbitration and editor-focused number shortcut as explicit boundaries. Existing reports. [forms-runtime-correction.md](forms-runtime-correction.md), [providers-settings.md](providers-settings.md), [input-focus.md](input-focus.md). Native boundary. Native Cobalt single-choice receipt verified; typing 2 in focused field inserted text rather than choosing option.

### C25. Timeline present-state, pending handoff and guest proposal rows

Show current attention/work/sync/goal, lifecycle and proposed next input at the end of conversation. Timeline footer, before queued own messages. Triggered by attentionNow/workNow/goalNow/sync, handoff, status line, guest or queued count.

Coexistence. Independent of top error lane and composer sending banners; Scroll to end summary can show child input/queue count. Responsive contract. NoticeText truncates with details popover while actions remain reachable; touch targets expand. Ownership. Selected thread; related-child open action names destination; guest send/dismiss are explicit.

Focus. Attention uses alert semantics, sync status; details popover exposes cut text, actions remain focusable. Motion. ThreadNow source has 150ms fade and reduced-motion opt-out. Lifetime. Footer mounted only while one present/queued condition exists; guest additions follow only if reader was already following.

Installed sources. [ChatView.tsx:3091](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3091), [ThreadNow.tsx:83](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadNow.tsx#L83), [ThreadActivityBanners.tsx:171](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadActivityBanners.tsx#L171), [GuestShareControls.tsx:220](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/GuestShareControls.tsx#L220).

Ferrite counterpart. Background-work footer line exists; Background panel uses separate inventory. [background_work.rs:112](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/background_work.rs#L112), [decisions.rs:45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/decisions.rs#L45). Wrong substitute. Persistent Background tab, top error toast or composer shelf is not same current-state position and reader-follow contract.

Prior coverage. Background and guest capability source coverage exist; combined footer ordering/coexistence was not audited as one composition. Audit omission or remaining boundary. Require current-state/queue visibility and named request destinations; optional guest runtime remains unexecuted. Existing reports. [subagents-background.md](subagents-background.md), [motion.md](motion.md), [coverage-quality.md](coverage-quality.md). Native boundary. Normal working/waiting/Stopped observed; guest/handoff/goal branches not native certified.

### C26. Composer sending notices and top error lane

Separate provider/thread faults from sending-context notices and machine attention. Provider/thread glass alerts at top of canvas; attached composer notice cards or detached below hero. Triggered by provider setup/error, thread failure; fallback, usage, clone, extension, fresh session, branch or environment item.

Coexistence. Question drawer can replace attention notices while machine warning/error remains; top lane independent. Responsive contract. Overflow NoticeText opens details; card action rows wrap; hero short-height rule places banners in flow. Ownership. Provider banner uses selected instance; thread error dismissal has own key/session state; composer items have stable IDs/order.

Focus. Refresh provider, setup, expand error and dismiss have separate controls; no focus theft by generic notification inferred. Motion. Composer stack fades 150ms with reduced-motion; top alert source does not prove equivalent motion. Lifetime. Ready/dismissed provider suppresses banner; pending refresh state settles independently of thread activity.

Installed sources. [ChatBannerLane.tsx:43](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatBannerLane.tsx#L43), [ProviderStatusBanner.tsx:107](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ProviderStatusBanner.tsx#L107), [ThreadErrorBanner.tsx:72](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadErrorBanner.tsx#L72), [ChatView.tsx:2552](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L2552), [ComposerBannerStack.tsx:28](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ComposerBannerStack.tsx#L28), [ChatComposer.tsx:3265](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3265).

Ferrite counterpart. Session errors/notices and composer error fields exist; exact two-lane equivalence not established. [background_work.rs:112](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/background_work.rs#L112), [pane.rs:881](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L881). Wrong substitute. One generic red message/toast cannot replace provider-refresh, thread fault and draft-specific actions.

Prior coverage. Provider banner real refresh observed; whole channel hierarchy previously fragmented across reports. Audit omission or remaining boundary. Important composition gap; source conditions must be exercised without overwriting draft or hiding recovery. Existing reports. [providers-settings.md](providers-settings.md), [input-focus.md](input-focus.md), [t3-files-terminal-live-source-match.md](t3-files-terminal-live-source-match.md). Native boundary. Codex probe timeout banner and visible recovery observed; root cause not diagnosed.

### C27. Plan, rendered HTML and MCP app artifact viewers

Read structured assistant artifacts and interact with their dedicated viewers. Plan card, HTML frame or MCP app iframe inline; HTML media dialog or MCP fullscreen on demand. Triggered by proposed-plan/html-render/mcp-app row kinds.

Coexistence. Fullscreen preserves inline height or pins parent state; unrelated tool panel remains distinct. Responsive contract. Plan content can collapse; HTML dialog caps viewport; MCP iframe changes display mode while preserving row height. Ownership. Plan save is workspace write with explicit form; HTML blob/fullscreen lifetime belongs viewer; MCP host scoped to app.

Focus. Plan action menu/save form, HTML expand/Close/zoom, MCP iframe interaction have separate focus contexts. Motion. HTML hover controls and shared modal styles exist; no MCP/native animation claim. Lifetime. HTML expansion releases fullscreen pin on unmount; closed MCP app shows recoverable closed state.

Installed sources. [MessagesTimeline.tsx:1573](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/MessagesTimeline.tsx#L1573), [TimelineAssistantRows.tsx:229](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelineAssistantRows.tsx#L229), [ProposedPlanCard.tsx:161](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ProposedPlanCard.tsx#L161), [HtmlRenderFrame.tsx:57](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/HtmlRenderFrame.tsx#L57), [HtmlRenderDialog.tsx:39](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/HtmlRenderDialog.tsx#L39), [McpAppFrame.tsx:437](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/McpAppFrame.tsx#L437).

Ferrite counterpart. Markdown/code/Mermaid exist; direct full artifact viewer equivalence not established. [rich.rs:535](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L535), [preview.rs:174](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L174), [transcript.rs:2912](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2912). Wrong substitute. Code fence, rendered file preview or embedded browser tab is not an interactive MCP app or artifact viewer.

Prior coverage. Markdown/media known; installed in-chat HTML/MCP modal composition not systematically reconciled. Audit omission or remaining boundary. Actual audit coverage omission, source-only; product adoption decision needed before counting as Ferrite defects. Existing reports. [transcript.md](transcript.md), [panel-tools.md](panel-tools.md). Native boundary. No native plan-save, HTML expand or MCP app execution was performed.

### C28. Secret and machine-link request cards

Resolve secret or machine-link requests in their dedicated UI. Secret inline form; link inline request launches multistep modal wizard. Triggered by secret_request or link_request item; display can become answered or pending elsewhere.

Coexistence. Separate from normal approval/question drawer and generic provider setup settings. Responsive contract. Forms wrap within message lane; link wizard uses shared modal presentation. Ownership. Request IDs and active owner; secrets use dedicated source/input rather than prompt transcript text.

Focus. Own form validation/submit/cancel; link wizard machine/access steps and failure state. Motion. Shared modal styles for link flow; secret form motion not established. Lifetime. Answered/pending-elsewhere renders compact row; in-flight buttons disabled.

Installed sources. [TimelineV2EventRow.tsx:19](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelineV2EventRow.tsx#L19), [SecretRequestCard.tsx:36](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/SecretRequestCard.tsx#L36), [LinkRequestCard.tsx:57](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/LinkRequestCard.tsx#L57), [dialog.tsx:18](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/dialog.tsx#L18).

Ferrite counterpart. General approval/elicitation machinery exists; no exact secret/link workflow counterpart established. [decision.rs:678](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/decision.rs#L678), [decisions.rs:45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/decisions.rs#L45). Wrong substitute. Settings secret provider entry or MCP reconnect control is not in-conversation request resolution.

Prior coverage. Settings capabilities named; these installed conversation branches were not explicitly mapped. Audit omission or remaining boundary. Actual surface inventory omission; source-only and optional, not a new runtime failure. Existing reports. [coverage-quality.md](coverage-quality.md), [providers-settings.md](providers-settings.md), [forms-runtime-correction.md](forms-runtime-correction.md). Native boundary. Neither branch was executed; no credentials or remote access changed.

### C29. Embedded Side chat conversation and Bring back

Explore a fork beside parent, then defer its context into next parent input. Tabbed workspace side pane with own title/Bring back/Delete/Close and embedded ChatView. Triggered by add Side chat or context action; finished side reply enables merge candidate.

Coexistence. Parent main conversation and side composer coexist; embedded view suppresses main header/details/workspace/drawer siblings. Responsive contract. Width is outer workspace contract; body reuses conversation lane/mobile rules. Ownership. Side thread differs from provider child; readSettings copies model/mode; merge records pending context transfer.

Focus. ComposerHandleContext reset isolates input owner; Bring back is a deferred operation, not auto-submit. Motion. No side-body interpolation proven; outer panel lifecycle belongs workspace map. Lifetime. Installed close requests delete onlyIfUnused, so a side chat with a run/message survives; explicit Delete is separate. Ferrite surface release removes the side tab and retires its backing thread. Native T3 reopened sent contents; restart/unsent-draft behavior remains unverified.

Installed sources. [SideChatPane.tsx:129](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/SideChatPane.tsx#L129), [useSideChat.ts:51](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useSideChat.ts#L51), [SideChatPane.tsx:59](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/SideChatPane.tsx#L59), [useRightPanelSurfaceClose.ts:73](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/useRightPanelSurfaceClose.ts#L73), [ChatView.tsx:3270](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3270), [ChatView.tsx:3832](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3832), [ChatView.tsx:3915](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3915).

Ferrite counterpart. Side chat and Bring back exist; Ferrite inserts serialized quote into parent draft instead of installed pending transfer design. [side_chat.rs:607](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L607), [side_chat.rs:776](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L776), [side_chat.rs:477](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L477), [side_chat.rs:308](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L308), [panel.rs:313](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L313), [side_chat.rs:802](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L802). Wrong substitute. A provider-child Agent panel or read-only compact transcript cannot replace independently editable Side chat.

Prior coverage. Real deferred receipt now verified; Ferrite controlled reopen failure known. Audit omission or remaining boundary. Keep main-child and side-pane authoring separate. Ferrite close/delete conflation is source-supported; the existing controlled blank reopen is runtime evidence, not a new native reproduction. Existing reports. [draft-lifecycle-runtime-correction.md](draft-lifecycle-runtime-correction.md), [subagent-placement-correction.md](subagent-placement-correction.md), [t3-live-source-correspondence.md](t3-live-source-correspondence.md). Native boundary. Actual inherited Sol context, next-parent receipt without sentinel supplied, and closed/reopened sent contents verified.

### C30. Thread-scoped confirmation and setup hosts

Confirm rewind/branch effects or start PR/device setup at the originating thread. Modal/alert-dialog hosts, not part of transcript rows or workspace tab body. Triggered by revert/Edit from here, restore branch, PR-thread action, device setup, Link PR.

Coexistence. Underlying chat can become inert while reverting; embedded mode suppresses most global setup hosts. Responsive contract. Shared dialog mobile sheet/max-height defaults; each host owns its content limits. Ownership. Overlay atoms carry threadKey; leaving route closes stale entries; handlers bind operation destination.

Focus. Dialog actions/cancel and shared primitive focus; no write is implied by merely opening dialog. Motion. Retained popup children support exit; source default 200ms opacity/scale, no runtime smoothness certified. Lifetime. Revert and image hosts remain independent; branch/PR/device/link hosts have embedded suppression.

Installed sources. [ChatView.tsx:3995](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3995), [ChatOverlayHosts.tsx:32](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatOverlayHosts.tsx#L32), [dialog.tsx:18](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/dialog.tsx#L18), [dialog-styles.ts:1](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/dialog-styles.ts#L1).

Ferrite counterpart. Rewind/fork and operation confirmation paths exist; exact per-host equivalence not established here. [preview.rs:174](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L174), [panel.rs:624](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L624), [decisions.rs:45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/decisions.rs#L45). Wrong substitute. One generic confirmation popup or panel tab does not prove correct thread ownership across route changes.

Prior coverage. Individual capabilities inspected; common overlay thread-lifetime contract was not captured. Audit omission or remaining boundary. Assembly/lifetime coverage omission; workspace and shell specialists own detailed tool dialogs. Existing reports. [composer.md](composer.md), [pr-operations.md](pr-operations.md), [coverage-quality.md](coverage-quality.md). Native boundary. PR actions inspected without remote writes; these destructive/remote confirmations were not executed.

### C31. Canvas preview, bottom terminal drawer and workspace siblings

Keep preview/tool work independently hosted beside the conversation. ThreadPreviewMiniPlayer inside canvas; ChatTerminalDrawers below column; ChatRightPanel beside column. Triggered by preview state, terminal toggle, workspace surfaces and maximize.

Coexistence. Details card is independent; maximized workspace explicitly makes chat inert; embedded side chat does not recursively mount workspace. Responsive contract. Inline/sheet/float/terminal-drawer are separate presentation contracts, delegated to workspace report. Ownership. Active threadKey and workspace store; preview session sync gated by capabilities and nonembedded route.

Focus. Separate focus scopes; hidden/maximized chat inert prevents accidental input. Motion. Panel duration passed explicitly; no claim that default 0ms yields animation. Lifetime. Presence retention and outer surface close/reopen owned by workspace reconciliation.

Installed sources. [ChatView.tsx:3270](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3270), [ChatView.tsx:3832](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3832), [ChatView.tsx:3915](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3915).

Ferrite counterpart. Ferrite Panel and terminal/browser/file surfaces exist. [panel.rs:624](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L624), [preview.rs:174](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L174). Wrong substitute. Thread details card, floating browser preview, terminal drawer and workspace tab are not interchangeable names.

Prior coverage. Source reports existed but header/card/tool spatial distinction caused actual omissions. Audit omission or remaining boundary. Conversation map records sibling boundary; full outer tool contract is delegated, not omitted. Existing reports. [thread-details-card-correction.md](thread-details-card-correction.md), [t3-surface-contracts.md](t3-surface-contracts.md), [panel-tools.md](panel-tools.md). Native boundary. Native browser/tool/terminal runs and one details coexistence recheck recorded by parent.

### C32. Voice recording and transcription state

Record, review and cancel spoken input without accidental send. Recording pill/button and live transcription inside composer; setup overlay when required. Triggered by speech capability/settings and user record/setup actions.

Coexistence. Question/approval modes and frozen editor have special submission ownership. Responsive contract. Resting/mobile composer may hide ordinary footer, but active voice keeps relevant controls visible. Ownership. Transcription environment and request generation separate from normal keyboard draft.

Focus. Cancel/finish shortcuts, setup focus and frozen-editor state must retain original input. Motion. No measured recording animation or meter timing; source controls only. Lifetime. Recording/preparing/transcribing/setup state governs editor freeze and cleanup.

Installed sources. [ChatComposer.tsx:3553](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3553), [ChatComposer.tsx:3591](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L3591).

Ferrite counterpart. No integrated voice UI established; OS dictation is a different path. [composer.rs:456](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L456). Wrong substitute. Audio attachment or operating-system dictation is not recording/transcription/cancel state in composer.

Prior coverage. Known optional voice gap; native audit observed Off and did not enable. Audit omission or remaining boundary. No new finding; explicit unexecuted conditional surface retained. Existing reports. [composer.md](composer.md), [coverage-quality.md](coverage-quality.md). Native boundary. Not executed; no microphone/model/integration started.

## Verification and remaining work

Only these two new reconciliation reports were written. This worker did not open apps, invoke models, manage runtime processes, run tests/typechecks/builds, or change product code. Source anchors were fetched from the exact installed revision and fingerprinted. Ferrite anchors were checked against local `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. The final parent-ledger snapshot SHA-256 is `ee2739e94e1d8d6b0fa3c924debebf8b6228f2c96d969af347bf33c80eb9630b`.

Prove It Works shaped the evidence split. Actual native observations establish only the described executions. Source establishes conditional composition and ownership. Existing controlled Ferrite replays establish only their protocol-backed public-control journeys. Unknown focus, motion, narrow-window, restart, or provider variants remain unknown. The report identifies the checks that were missing from audit composition; it does not label those unexecuted checks as product failures.

Related surface maps. [surface-reconciliation-completeness.md](surface-reconciliation-completeness.md).

The workspace specialist owns the detailed outer panel, tool drawer, modal, and tab map. This report records their ChatView mount and embedded-suppression boundary so those surfaces cannot disappear from the overall reconciliation. Parent reconciliation should join records by actual opener and destination, with the selected environment/thread/request or artifact identity held constant through open, coexistence, resize, close, and return.
