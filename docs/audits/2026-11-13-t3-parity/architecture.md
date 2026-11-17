# Architecture behind Ferrite's interaction failures

## Overview

The user wants Ferrite to support complete, dependable work without requiring them to enumerate every broken click, transition, citation, or recovery path. This synthesis explains why many implemented features still fail that standard. The detailed issue inventory belongs to the slice reports. [Remediation](remediation.md) turns these architectural findings into verifiable work.

The central problem is incomplete integration across ownership boundaries. A panel tab can exist without the resource that renders its body. A browser can own keyboard focus while the cockpit owns its Enter action. A PR link can identify another thread without making that thread visible. A quote can retain its words while losing its source, editability, or status as inert reference text. These conclusions follow the traced source paths below. They do not rely on the number of implemented enums, handlers, or tests.

This does not justify rewriting Ferrite. Several existing boundaries already encode useful invariants. Terminal input has explicit action ownership, browser control rejects stale operator revisions, AgentKey preserves provider-scoped identity, file previews hydrate from their durable identity, and motion retargets from the displayed value. Those mechanisms are candidates for reuse because of the specific invariants they enforce, not because their presence proves complete product quality.

Source claims refer to Ferrite `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 comparison links pin `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. This document makes source-backed architectural judgments. It adds no runtime reproductions, test results, severity counts, or performance measurements. The parent maintains those results in the audit's evidence and consolidated report. The independent review's statement that runtime access had wholly failed predates the recovery described in [runtime-access.md](evidence/runtime-access.md).

## Key concepts

### A durable identity is different from the currently visible view

A Thread owns history and sessions. A Subject identifies Main or a child agent within that Thread. A Pane displays a Thread and Subject. A `SurfaceId` identifies a tab in that Thread's right panel. Focus, selected pane, selected surface, and allocated resource are separate facts. `panel_candidate_index` chooses only the focused pane, while `ThreadSurfaces` stores its own active identity and tab order. See `crates/ferrite-cockpit/src/cockpit/panel.rs:181-211` and `crates/ferrite-cockpit/src/cockpit/surfaces.rs:79-95`.

Navigation must carry the complete destination and an explicit visibility policy across these states. Background data arriving for a Thread must not acquire the same authority as an operator choosing that Thread. Ferrite already distinguishes operator revisions from agent reveals in `ThreadSurfaces::reveal`. That invariant should survive any navigation repair. See `crates/ferrite-cockpit/src/cockpit/surfaces.rs:112-165`.

### Focus is an action-routing contract

GPUI dispatches key-bound actions before raw key listeners. An action consumed in the first phase prevents the raw browser or launcher handler from seeing the key. Separately, the cockpit render path can move focus back to its composer. Correct ownership therefore requires both the action context and redraw preservation to agree. See `vendor/gpui-pre/src/window.rs:6152-6168,6539-6557` and `crates/ferrite-cockpit/src/cockpit.rs:10109-10274`.

A mouse handler on a `Div` is insufficient for keyboard or accessibility behavior. GPUI requires explicit focus and role support. Existing form controls and menus demonstrate the native mechanisms available. See `vendor/gpui-pre/src/elements/div.rs:1876-1882,2222-2241,2992-3057`, `crates/ferrite-ui/src/components.rs:144-158`, and `crates/ferrite-ui/src/menu.rs:897-910,1025-1043`.

### A compact projection is not the complete domain record

Ferrite frequently has more data than its current view exposes. `WirePr` contains review discussions that the new Timeline reduces. Activity stores ancestry and coverage that the compact Agent transcript omits. Transcript search reads the rendered row projection, so closed folds remove searchable material. Diff rendering caps the visible result even after the backend loads the full diff. See `crates/ferrite-core/src/protocol/review.rs:575-610`, `crates/ferrite-cockpit/src/cockpit/side_chat.rs:477-574`, `crates/ferrite-pane/src/transcript.rs:1201-1247`, and `crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:334-425`.

A compact summary can be appropriate. It becomes incomplete when the user has no path to the omitted material or when a summary is reused as the authoritative search, review, or control model.

### Saved presentation state is different from recoverable work

`panel.json` can atomically store tab identities without storing the Files working set or reconstructing Agent and SideChat resources. A saved draft string can retain review comment words without restoring their editing controls. A saved attachment path can outlive its temporary file or refer to different bytes later. See `crates/ferrite-cockpit/src/cockpit/surfaces.rs:342-405`, `crates/ferrite-cockpit/src/cockpit/panel.rs:124-141`, `crates/ferrite-cockpit/src/cockpit/drafts.rs:28-56`, and `crates/ferrite-pane/src/composer.rs:2104-2124`.

Recovery requires a defined owner, a durable record, and a restore operation. A persisted outer shell alone does not meet that requirement.

## How it works

### Input reaches a global command before the intended control

Browser content focuses its own handle, but it lives inside the Reader context and forwards keys through raw listeners. The global Ferrite bindings include Submit, Tab movement, and permission-mode cycling. Reader binds Escape to close. That ordering explains why a focused page can still submit the main draft or close the preview. The empty-panel launcher makes the same action-order assumption. Files has a second failure route. Its edits notify the cockpit, whose focus reconciliation lacks a Files editor or filter owner and can return typing to the main composer. See `crates/ferrite-pane/src/visual/element.rs:265-289,645-663`, `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:498-520`, `crates/ferrite-ui/src/keymap.rs:179-192,304-315,537`, `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:670-742`, and `crates/ferrite-cockpit/src/cockpit/tools/files.rs:228-239,446-464`.

Close and Find expose the same incomplete migration. They still dispatch through special cases for older preview and review owners instead of the active panel tool. A terminal or PR can fall through to closing its conversation. Files Find can fall through to searching the transcript. See `crates/ferrite-cockpit/src/cockpit.rs:9148-9175,6144-6164`.

The architectural repair is explicit ownership of commands by the focused area and surface, with a deliberate escape route back to the workspace. Terminal's existing context already consumes Enter, Tab, Escape, and clipboard actions before the cockpit. It provides a reusable native example. T3 expresses the corresponding boundary with [workspace area focus](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/workspaceFocus.ts#L175-L257) and [guest keyboard handling](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L1509-L1583). Ferrite's reusable implementation is `crates/ferrite-terminal/src/view.rs:324-344` and `crates/ferrite-ui/src/keymap.rs:604-617`.

Adding another raw handler or another focus exception would address individual symptoms while preserving the cause. That is a design judgment based on the repeated browser, launcher, Files, Find, and Close failures.

### Navigation resolves a URL but loses destination ownership

The sidebar PR badge calls `open_pr(thread)`, which submits a Browser open for that Thread. The badge stops the enclosing card's activation. Browser reveal acts on the requested Thread, but the visible panel still belongs to the focused pane. The source therefore permits a successful hidden open that appears to do nothing. Native PR entry points use a separate route through `open_pr_surface`. See `crates/ferrite-cockpit/src/cockpit.rs:1826-1831,12806-12819`, `crates/ferrite-cockpit/src/cockpit/browser.rs:637-647`, and `crates/ferrite-cockpit/src/cockpit/panel.rs:254-266`.

The target model is also too narrow for independent native PR navigation. `PullRequest(u64)` is scoped under a Thread, while its detailed snapshot is read from that Thread's current checkout. The lightweight facts URL, native snapshot, and action prompts can consequently express different aspects of the target. T3 uses [repository-scoped PR identity](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L267-L298) and [an opener that activates the target context](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/openPullRequestLink.ts#L74-L137). Ferrite's definitions and reads are `crates/ferrite-cockpit/src/cockpit/surfaces.rs:43-45` and `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:159-205,274-305`.

Link menus repeat the identity problem at a smaller scale. Plain-click issue references resolve `issue:212`, but menu Open and Copy use the renderer's literal string. A right-click on a PR badge can inherit the enclosing Thread menu and copy a Thread link. File chips replace native linked text with custom elements that do not carry the same menu metadata. See `crates/ferrite-pane/src/rich.rs:567-575,617-644`, `crates/ferrite-pane/src/transcript/message_menu.rs:192-203,269-284`, `crates/ferrite-cockpit/src/cockpit/nav_list.rs:339-344`, and `crates/ferrite-cockpit/src/cockpit/context_menu.rs:828-829`.

A shared resolved target must drive activation, menu identity, clipboard content, and keyboard behavior. The destination policy can still distinguish native detail, integrated browser, system browser, file location, Thread, and document fragment. T3's [link destination policy](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/browserLinkTarget.ts#L11-L27) is comparative evidence for explicit policy, not proof that every T3 caller forwards every modifier correctly.

### New panel views bypass older complete flows

The normal Agent row opens a compact panel drill. The older full-Pane route calls `ensure_subject_history`, records visibility, carries child identity, and supplies richer transcript behavior. The compact route reads retained blocks without requesting missing history, and its event handler accepts only `ToggleDisclosure`. Rendering also omits decisions and nested agent mappings. A retained live child can look correct while an evicted child or nested interaction fails. See `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:80-143`, `crates/ferrite-cockpit/src/cockpit/subagents.rs:376-438`, `crates/ferrite-cockpit/src/cockpit/side_chat.rs:477-574`, and `crates/ferrite-core/src/engine/history.rs:110-172`.

The PR Timeline has a related projection fault. The backend normalizes review roots, replies, resolution, and quoted hunks. The new panel's presentation drops parts of that record, while the older Review slot already renders more of it. The Changes panel similarly reuses an abbreviated hunk renderer for a full review destination. See `crates/ferrite-reviewr/src/forge.rs:1025-1130`, `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:689-840`, `crates/ferrite-cockpit/src/review.rs:1898-1953`, and `crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:271-425`.

The appropriate consolidation is a shared domain presenter and event contract for each tool. Main and compact views may differ in layout, but must preserve target identity, supported actions, completeness markers, and history loading. Reusing an entire compact widget without its missing contracts is the demonstrated failure mode. Reusing the provider adapters and normalized domain records avoids a needless rewrite.

### Content becomes an ambiguous string before its lifecycle ends

The Composer stores files, replies, review comments, annotations, and tokens separately, then `prompt()` flattens them. `take()` clears all of them. Existing-thread drafts persist the flattened string and `set()` parses it back. `Reply` itself contains only quote and text, with no source identity. Its serializer flattens comment newlines, and its parser treats leading Markdown blockquotes as structured replies. See `crates/ferrite-pane/src/composer.rs:494-554`, `crates/ferrite-pane/src/quote.rs:50-118`, and `crates/ferrite-cockpit/src/cockpit/drafts.rs:28-56`.

That choice couples several visible defects. Literal prose can become a noneditable citation on recall. A quote and an unrelated file can render differently after send because decode order differs. Review controls do not survive draft restoration. Quoted `@path` and skill syntax can become active provider context because adapters scan the combined text. See `crates/ferrite-pane/src/pane.rs:3226-3241,3543-3557`, `crates/ferrite-core/src/prompt_files.rs:49-93`, `crates/ferrite-core/src/providers/claude/wire.rs:25-62`, and `crates/ferrite-core/src/providers/codex/wire.rs:1242-1293`.

Cite also exposes an authoring gap independent of serialization. The selection handler creates `Reply::new(passage, "")`, and the card has no comment editor. A test that injects `Reply.text` proves rendering only. General composer text remains available in source, so "the user cannot type anything after citing" is not established by this inspection. See `crates/ferrite-pane/src/transcript/reply.rs:206-220`, `crates/ferrite-pane/src/attachments.rs:551-618`, and `crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:954-975`. The [quote supplement](reports/quote-attachments.md) corrects the earlier composer report on this point.

The required boundary is a structured draft and send snapshot that retains source, user comment, attachment identity, ownership, and preparation status until the provider adapter. Plain text remains a valid provider output, but should not be the only source of truth during editing and recovery. T3 illustrates that distinction with its [citation schema](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/assistantCitations.ts#L11) and [separate dispatch preparation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1292).

### Operations retire local work before their outcome is known

Idle Submit takes the Composer before calling a unit-returning send API. Some admission failures return before the normal delivery-failure recovery. File close removes a tab before attempting its save, whose error is silent. A cached file buffer also writes without detecting another actor's edit. These are different operations with the same missing rule: local work remains recoverable until acceptance or persistence is confirmed. See `crates/ferrite-cockpit/src/cockpit.rs:5552-5596`, `crates/ferrite-core/src/engine.rs:2537-2578`, `crates/ferrite-cockpit/src/cockpit/tools/files.rs:207,256-263`, and `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:135-141,219-227`.

PR operations add a freshness problem. Lightweight status and detailed snapshots have separate caches. Detail invalidation compares only number and tone. Agent-backed mutations lack a panel-owned operation outcome that can invalidate the appropriate data. A forced read replaces the good snapshot with Reading, so transient failure removes useful information. See `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:283-335` and `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:358-380`.

The remedy is explicit operation state, including target, pending attempt, result, retry, and uncertain delivery. Do not retry an uncertain send as though it were definitely rejected. Do not serialize every operation globally. Changes already rejects stale read completions with a generation counter, and that local pattern is applicable to PR reads. See `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:260-394`.

### Resource lifetime disagrees with close and restore policy

The panel marks Agent and SideChat tabs reopenable, then closes their resources. Reopen restores only `SurfaceId`. Agent and SideChat body renderers return empty elements when those resources are missing. File previews differ because `sync_previews` rebuilds them from the active path. See `crates/ferrite-cockpit/src/cockpit/surfaces.rs:48-53`, `crates/ferrite-cockpit/src/cockpit/palette.rs:1294-1298`, `crates/ferrite-cockpit/src/cockpit/panel.rs:314-319`, `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-207`, `crates/ferrite-cockpit/src/cockpit/side_chat.rs:308-324,607-615`, and `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:298-319`.

Browser service metadata and native page lifetime are intentionally separate. Restoring a tab does not prove a live native Page exists. Control eligibility also depends on owner, manual revision, pending navigation, document identity, and loading state. That explains why a generic stale-document rejection cannot identify the original runtime failure by itself. See `crates/ferrite-cockpit/src/cockpit/browser.rs:104-126,649-678,1063-1065` and [runtime-access.md](evidence/runtime-access.md).

External websites also share CEF policies intended for sealed visualization content. Unconditional popup refusal and JavaScript-dialog dismissal remove ordinary web workflows without a visible recovery route. Preserve sealed-content restrictions while giving external sites an explicit, separate policy. See `crates/ferrite-pane/src/visual/web/cef/handlers.rs:151-165,384-450,643-661`.

### Environment identity is missing from some management operations

The window can connect to a remote server, but Settings writes the local file and then requests a reload on the active server. Provider management probes or updates local binaries while invalidating the active server's sessions. Permission cards can display global defaults instead of the running session's effective sandbox. These are concrete ownership errors, not evidence that SSH or provider integration is absent. The settings path is `crates/ferrite-cockpit/src/cockpit.rs:4319-4364` and `crates/ferrite-core/src/server.rs:784-789,1042-1053`. The management paths are `crates/ferrite-cockpit/src/cockpit.rs:4083-4115,12968-12975` and `crates/ferrite-core/src/providers/update.rs:214-231`. The permission mismatch crosses `crates/ferrite-cockpit/src/cockpit/decisions.rs:65-79` and `crates/ferrite-core/src/providers/codex.rs:504-522,1048-1055`. [providers-settings.md](reports/providers-settings.md) traces those ownership errors in PS-01 through PS-03.

Management requests need an explicit target environment, and session facts need to remain distinct from defaults for future sessions. T3's separate environment and provider-instance model demonstrates that distinction. Ferrite does not need T3's extra providers or relay product to repair its existing local and SSH flows.

### Motion quality depends on state ownership and layout stability

Ferrite already has retargetable tweens, reversible reveals, retained exits, row movement, and some smooth programmatic scrolling. Sidebar and panel widths nevertheless snap by explicit policy. Browser loading inserts content and changes geometry. Panel bodies often replace directly. Existence of `Tween` cannot establish coverage of these transitions. See `crates/ferrite-ui/src/motion.rs:17-70,923-1042,1335-1365`, `crates/ferrite-ui/src/theme.rs:3531-3574`, and `crates/ferrite-cockpit/src/cockpit/browser.rs:1491-1540`.

Scroll has a deeper ownership fault than missing animation. Native tail follow, explicit easing, sent-prompt anchoring, synthetic bottom room, Find, and resize correction can each position the viewport. A track click does not set the flag that cancels easing. A thumb drag after easing can leave the sent-prompt anchor alive, so the first tool can pull the reader back to the end. Find revokes earlier ownership only for a nonoverlapping row, which fails when a tall row intersects the viewport. See `crates/ferrite-ui/src/scrollbar.rs:871-905`, `crates/ferrite-pane/src/transcript/scroll.rs:163-169`, `crates/ferrite-pane/src/transcript.rs:1033-1070,3017-3023`, and `crates/ferrite-pane/src/transcript/find.rs:40-51,136-172`.

The models also disagree about geometry. End retains synthetic room that the latest button clears. The latest cue subtracts padding from an extent that already excludes it. Page travel ignores the pinned prompt's covered area. Pending ease, runway, and resize targets use numeric row indices separately from the native list's splice adjustment. The missing resets and rebasing are source-defined, while a particular stale-target landing still requires a concurrent runtime transition. See `crates/ferrite-pane/src/transcript.rs:1073-1091,2204-2229,2323-2331,3035-3062`, `crates/ferrite-pane/src/transcript/scroll.rs:128-133,316-324`, and `vendor/gpui-pre/src/elements/list.rs:537-547,878-883`.

A single viewport owner, qualified by transcript generation and stable row identity, can coordinate these operations without replacing the native list. Manual navigation must revoke pending automatic movement even when no tween exists. T3 supplies comparative examples in its [navigation signal](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.ts#L86-L155) and [identity-based restoration](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L124-L144). The existing Ferrite native list already rebases pending measurements and freezes scrollbar geometry during a drag. Preserve those mechanisms at `vendor/gpui-pre/src/elements/list.rs:739-752,931-935,1377-1416`.

Long and unmeasured jumps have additional explicit snap fallbacks at `crates/ferrite-pane/src/transcript/scroll.rs:159-203`. Neither those fallbacks nor the new source findings identify every cause of the user's intermittent symptom. [scroll-quality.md](reports/scroll-quality.md) distinguishes proven control-flow defects, inferred reflow risks, and the real gesture checks needed to settle them. A better easing curve alone would leave the competing viewport owners intact.

The user's request establishes useful motion as a Ferrite requirement. T3's [zero default panel duration](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/settings.ts#L155-L164) does not negate it. Repair ownership and stable geometry first, then specify intermediate states, interruption, focus, reduced motion, and completion for each transition. Animated screenshots alone cannot prove those contracts.

## Where things live

| Boundary | Owning code and relevant reuse |
| --- | --- |
| Input routing and focus | `crates/ferrite-ui/src/keymap.rs`; `crates/ferrite-cockpit/src/cockpit.rs:10109-10274`; native Terminal actions in `crates/ferrite-terminal/src/view.rs:324-344`. |
| Navigation and panel lifecycle | `crates/ferrite-cockpit/src/cockpit/surfaces.rs`; `cockpit/panel.rs`; `cockpit/palette.rs:1294-1298`. Preserve operator-revision reveal checks. |
| Link target resolution | `crates/ferrite-pane/src/rich.rs`; `file_links.rs`; `transcript/message_menu.rs`. Preserve selection and streaming click protections in `vendor/gpui-base/src/text/inline.rs:64-87,1176-1241`. |
| Agent identity and history | `crates/ferrite-core/src/activity.rs:15-76,2186-2249`; `engine/history.rs:110-172`; compact and full routes in `cockpit/agent_pane.rs` and `cockpit/subagents.rs`. |
| PR domain and presentation | `crates/ferrite-core/src/protocol/review.rs`; `crates/ferrite-reviewr/src/forge.rs`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs`; older complete discussion rendering in `review.rs:1898-1953`. |
| Drafts, quotes, attachments, provider expansion | `crates/ferrite-pane/src/composer.rs`; `quote.rs`; `attachments.rs`; `crates/ferrite-core/src/prompt_files.rs`; provider wire modules. |
| File and diff tools | `crates/ferrite-cockpit/src/cockpit/tools/files.rs`; `editor.rs`; `changes.rs`; `cockpit/panel/hunks.rs`. Reuse Changes generation checks and File preview hydration. |
| Browser service and native page | `crates/ferrite-core/src/browser.rs`; `crates/ferrite-cockpit/src/cockpit/browser.rs`; `crates/ferrite-pane/src/visual/web/cef/handlers.rs`. Preserve manual-control protection and profile ownership. |
| Motion and scrolling | `crates/ferrite-ui/src/motion.rs`; `crates/ferrite-pane/src/transcript/scroll.rs`; `crates/ferrite-cockpit/src/nav/flip.rs`. Verify caller behavior as well as shared helpers. |
| Verification | `crates/ferrite-cockpit/src/cockpit/tests`; `crates/ferrite-cockpit/src/headless.rs`; `.github/workflows/ci.yml`; `.github/workflows/browser-window.yml`. |

Paths shortened in a row retain the directory of the preceding full path where unambiguous. The detailed reports supply full references for individual findings.

## Gotchas

- Ordinary PR CI omits the cockpit and pane interaction suites, but a browser-specific workflow runs a narrow subset. "There are no UI tests" is false. A direct helper test can also pass while production key dispatch prevents the helper from running. See `.github/workflows/ci.yml:87-93,124-133`, `.github/workflows/browser-window.yml:14-30,219-237`, and `crates/ferrite-cockpit/src/cockpit/tests/tools.rs:190-220`.
- Native GPUI driver events can prove host interaction routing. A build without CEF cannot prove actual Chromium behavior, operating-system composition, or popup handling. The exact feature set and source commit must accompany runtime evidence. See `crates/ferrite-cockpit/Cargo.toml:11-23` and [runtime-access.md](evidence/runtime-access.md).
- The evicted-Agent history defect requires missing retained history. It does not imply every live child opens blank. File stale-write protection is a Ferrite quality requirement even though the independent review found no equivalent protection in T3's cited save modules.
- Background work has several projections and entry points. The investigation found explicit opening paths and notification-driven subject selection, not a proven unsolicited event that always switches to Background. See `crates/ferrite-cockpit/src/cockpit.rs:12274-12327` and [subagents-background.md](reports/subagents-background.md).
- A URL link has source-defined Open and Copy menu rows in this baseline. The user's Copy-only observation needs its exact target and build. Do not dismiss that observation, or claim that every link context has a correct Open action. See `crates/ferrite-pane/src/transcript/message_menu.rs:278-282` and [link-affordances.md](reports/link-affordances.md).
- Persisting attachment paths is an explicit historical choice in `docs/adr/0004-prompt-attachments.md:23-36`. Improving retained media and stale-reference feedback requires a documented migration. It must not silently change ordinary workspace mentions into frozen copies.
- Additional provider drivers, guest participation, device panels, voice, rich composition, and terminal splits remain product options. They do not block the essential repair of currently offered input, navigation, citation, save, and recovery behavior.

The Model the Domain principle shaped the recommendation to encode focus ownership, resolved targets, structured content, and resource lifecycles at their existing boundaries. Sequence Work into Verifiable Units shaped the separate repair program. Neither principle requires replacing the rendering framework, provider stack, or entire cockpit.
