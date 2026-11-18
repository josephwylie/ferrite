# Production-quality repair program

The user wants complete, reliable Ferrite workflows, useful motion, and readable context without supplying a defect checklist. This program repairs the shared causes identified in [architecture.md](architecture.md), then completes the affected journeys. It is a proposal for subsequent implementation, not a claim that repairs, tests, or releases occurred during this audit.

The sequence prioritizes unintended actions and lost work, correct navigation, content identity, complete tool behavior, recovery, scroll ownership, and presentation. Every unit ends in an observable result. The detailed reports remain the source for issue IDs and evidence. The parent owns consolidated counts and runtime adjudication.

## Corrective runtime gates

The first audit was insufficiently grounded in the everyday journeys the user described. The corrective pass adds concrete regression gates to the existing units below. These are repairs to existing behavior and presentation, not an expansion into optional T3 capabilities. [52-journey coverage matrix](reports/workflow-coverage-correction.md).

| Existing unit | Reproduced gate that must turn green |
|---|---|
| 05: destination identity | With A visible, click B’s actual sidebar PR badge. The intended PR must become visible and every entry point must agree on the target. [Two-repository fixture](reports/pr-entry-runtime-correction.md). |
| 07: draft persistence | Preserve a never-sent draft through the documented restart contract, and edit a queued prompt without silently combining it with unrelated draft text. [Runtime evidence](reports/draft-lifecycle-runtime-correction.md). |
| 08: readable citations | A long prose/code quote must be identifiable and fully inspectable before sending; authoring a comment must be reachable from the actual UI. Preserve the successful full-payload behavior. [Runtime evidence](reports/attachment-runtime-correction.md). |
| 10: PR freshness and recovery | Change checks, description and comments without changing PR number/lifecycle, then use both visible Refresh routes. Overview and detail must agree. Render long read errors with wrapping and usable recovery. [Fixture and red verdicts](reports/pr-entry-runtime-correction.md). |
| 12: ordinary Agent route | Direct child opening and View live output must reveal the same current work. Nested child links must work, and prose must remain complete at 420px as well as maximized. Preserve the working main/child incremental paths. [Runtime evidence](reports/agent-live-correction.md). |
| 13: reopen | Close/reopen Agent and SideChat from actual tab controls. Restore content, identity and available composer, or present a truthful nonblank recovery state. [Agent](reports/agent-live-correction.md), [SideChat](reports/draft-lifecycle-runtime-correction.md). |
| 14 and 18: browser composition | Remove unjustified duplicate browser headings/tab hierarchy, make empty address entry recognizable, and preserve toolbar/page geometry through loading. Compare native empty/loading/ready/error states at the same width. [Visual correction](reports/visual-quality-correction.md), [native observations](evidence/native-correction.md). |
| 15: files | Refresh clean preview and tree after external changes; all 18 kept tabs and close controls must remain reachable. Do not count that check as proof of dirty-save safety. [Runtime evidence](reports/files-runtime-correction.md). |
| 18: panel sizing and motion | A Files/Changes visit must not replace an explicitly chosen compact width with a persistent 60%-window width for unrelated surfaces. Verify close/open/restart, actual divider drag and narrow widths. Implement visible open/close motion, interruption and reduced-motion behavior for both sidebar and panel. Static screenshots cannot establish animation. [Runtime sizing](reports/panel-runtime-correction.md), [T3 source contracts](reports/t3-surface-contracts.md). |
| 20: navigation and search | Reveal the selected search result before Enter can open it; Snoozed and Parked Rename must show a real editor. Search must find and reveal folded tool content. [Navigation](reports/navigation-runtime-correction.md), [folded Find](reports/scroll-runtime-correction.md). |

Do not use the inconclusive paging captures, form-input captures or Working rename commit as red product tests. First establish actual viewport/input state and settled persistence. Actual CEF keyboard/IME, manual scroll interruption, image intake and pending decisions remain priority native verification gaps. Those are not passes, and their absence from this runtime sample is not evidence that the workflows are reliable.

## Live T3 implementation references

Use the [rendered-surface reconciliation](reports/surface-reconciliation.md) alongside feature-level traces. A destination's placement, coexistence, focus owner, close/reopen behavior and responsive lifetime are part of its acceptance contract. A similarly named tab is not sufficient evidence of equivalent behavior.

The [floating Thread details card correction](reports/thread-details-card-correction.md) adds a separate requirement: lightweight thread context must be independently accessible alongside the full tool workspace. Preserve separate visibility, responsive presentation and focus behavior. Overview occupying the tool panel is not equivalent. This is `quality-correction-005`, distinct from the saved-width defect in Unit 18.

The subsequent comparison ran real Sol and Sonnet tasks in installed T3 `1264c206aee0dce805d6279809118bc091f29d2f`. Use the [live conclusions](reports/t3-live-conclusions.md) and exact installed source traces when implementing these units: [conversation, citations, queue, agents and provider output](reports/t3-live-source-correspondence.md); [PR identity, panel sizing and motion](reports/t3-pr-live-source-match.md); [browser input, chrome and lifecycle](reports/t3-browser-live-source-match.md); [files, search and terminals](reports/t3-files-terminal-live-source-match.md). The newer comparison checkout is 746 commits ahead and remains a separate reference.

The working T3 examples establish practical targets: an actual cross-thread PR click opens the clicked PR; both providers expose child stdout before completion; a citation comment and image reach the real provider together; Side chat delivers its context on the next parent turn and survives close/reopen. These are reference observations, not passing Ferrite gates. Preserve the required basic link actions and desired motion even where T3 itself is limited. The [coverage matrix](reports/t3-live-coverage-reconciliation.md) identifies the reference variants that remain unexecuted.

## Delivery rules

Use the existing GPUI application, provider integrations, native list, and motion machinery. Refactor the ownership boundary implicated by each unit. Do not begin with a cockpit rewrite or add a second framework around the existing state. Terminal action overrides, `AgentKey`, history hydration, `ThreadSurfaces` operator revisions, Changes generation checks, and preview hydration are concrete reuse candidates described in [architecture.md](architecture.md).

For each unit, record the trigger, the current observable failure, the desired result, the narrow verification, and any remaining native-platform gap. Where the source result is inferred, first reproduce it in the unit's fixture. A result that does not reproduce is evidence to narrow the repair, not permission to relabel it as proven.

Use a fresh temporary Store, scratch checkout, deterministic provider, and local website fixtures. Record the source commit, build features, operating system, and exact commands. Keep application-level tests separate from actual CEF and operating-system input results. Never use a live unsent user draft to probe unintended submission.

Respect the repository verification policy. Run the narrowest relevant Rust tests with `cargo nextest run -p <crate>` and a filter for the changed files or behaviors. After the last edit, one green narrow run and one applicable typecheck complete that part of verification. Run the relevant E2E flow once before handoff for a user-flow change. Do not rerun a delegate's reported green command or run the full suite locally by default. Build with plain Cargo and inspect free disk with `df`. Do not set `CARGO_TARGET_DIR` for these units.

The numbered units below describe delivery order. Some later units can proceed after their stated dependencies without waiting for unrelated work. Each must still land with its own acceptance evidence. The labels are work-package identifiers, not new defect counts.

## Establish the verification boundary

### Unit 00. Make the real entry paths reproducible and gated

Keep existing handler and domain tests, then add fixtures that click rendered controls and dispatch keys through production bindings. The smallest fixture set needs two Threads with different PR targets, one pending approval, a nested Agent, scratch files, a long transcript, and a local browser page. The page must have editable fields, same-tab and new-window links, JavaScript dialogs, and a controllable loading failure.

Add changed-area cockpit and pane checks to ordinary PR CI. Keep the native browser workflow, but trigger its relevant gate when browser host, input, or lifecycle code changes. A synthetic engine pass must remain visibly distinct from a CEF pass. Verify valid provider-form entry as well as invalid rejection.

Acceptance requires a deliberately broken binding or swallowed click to fail the interaction test. Typing a valid form value must send that exact typed value and leave the main draft unchanged. CI must report the actual selected tests and feature set. Existing `.github/workflows/ci.yml:87-93,124-133`, `.github/workflows/browser-window.yml:14-30,219-237`, and `crates/ferrite-cockpit/src/cockpit/tests/provider_forms.rs:306-359` define the current gap.

This unit starts the program, but does not require a comprehensive new E2E suite before individual repairs.

## Prevent unintended actions and lost work

### Unit 01. Give the focused control ownership of its keys

Repair action routing for browser content, the empty-panel launcher, Files editor, and Files filter. Use a native input context and a focused-area owner. Preserve that owner across redraws. Submit must require the intended sending control or an explicit submit command. Ordinary typing in an empty approval composer must not trigger bare-letter permission actions.

Acceptance uses a sentinel draft and separate key events across provider updates. Browser Enter, Tab, Shift-Tab, Escape, Copy, and Paste act on the page under the documented host-shortcut policy. The launcher opens its highlighted tool. File typing and filtering remain in their controls. Typing "also", "no", and "yes" under a pending approval changes text and sends no decision. Explicit approval controls still work.

Verify host dispatch with production keys, then verify the browser branch in a CEF build. Composition belongs to Unit 14 and must remain an explicit gap until tested. Sources are [input-focus.md](reports/input-focus.md), `vendor/gpui-pre/src/window.rs:6152-6168`, and `crates/ferrite-cockpit/src/cockpit.rs:10109-10274`. Preserve Terminal's existing ownership behavior.

### Unit 02. Retain a complete draft until send admission resolves

Make send admission return a definite outcome. Retain the full draft snapshot until acceptance or keep a recoverable pending record with a request identity. Restore definite refusal without overwriting newer typing. Represent an uncertain transport result separately so Retry does not duplicate an already accepted message.

Acceptance covers an unavailable workspace, bootstrap in flight, a provider start refusal, ordinary delivery failure, success, and interruption during admission. Text, quotes, files, annotations, and review context survive every definite refusal. With newer typing, the failed snapshot remains recoverable as separate work. A successful request appears once. The visible pending or error state names the affected request and has a working recovery action.

This can initially snapshot the existing Composer data without waiting for the final structured-draft migration. Unit 07 removes the string-only persistence limitation. Sources are `crates/ferrite-cockpit/src/cockpit.rs:5552-5596`, `crates/ferrite-pane/src/composer.rs:516-527`, and `crates/ferrite-core/src/engine.rs:2537-2578`.

### Unit 03. Make file save and close preserve recoverable edits

Keep an edited buffer until its write succeeds or the user deliberately discards it. Expose Saving, Saved, and Failed states. Detect a changed disk version before overwriting it and offer a defined reconcile or reload path. Scope each save result to the file identity and edit revision so a late response cannot mark newer edits saved.

Acceptance uses a temporary checkout and injected write refusal. Close preserves the unsaved text and presents Retry or Save elsewhere. Restoring write access and retrying saves the same content. An external edit made after opening cannot disappear when the user types into the old buffer. A late successful write cannot clear a newer dirty revision. Reopening reads the confirmed disk version.

The test must click and type into the real editor after Unit 01, not invoke `Editor.type_text` as the sole proof. Sources are `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:135-141,219-227` and `crates/ferrite-cockpit/src/cockpit/tools/files.rs:207,256-263`. T3 parity is unnecessary to justify safe writes.

### Unit 04. Keep quoted reference text inert at the provider boundary

Prevent quoted file paths and skill syntax from becoming active attachment or skill input. Preserve the exact quotation as reference text. Keep deliberate user-authored mentions and explicitly attached context active. Do not escape all prompt text or silently change the quoted words.

Acceptance captures the real adapter output for both providers using harmless fixture files. Citing `@path` syntax and a discovered skill name adds no unsolicited file bytes, image/audio item, mention, or skill invocation. Explicitly attaching the same fixture still creates the correct typed input. Repeat after draft restoration and retry.

This is an immediate containment repair before Unit 07 carries typed context through the entire lifecycle. Sources are `crates/ferrite-pane/src/quote.rs:74-85`, `crates/ferrite-core/src/prompt_files.rs:49-93`, the provider wire modules cited in [quote-attachments.md](reports/quote-attachments.md), and the existing annotation separation at `crates/ferrite-core/src/browser_annotations.rs:453-461`.

## Make navigation and controls agree on their targets

### Unit 05. Resolve one destination for every activation of a link

Define a resolved target that preserves repository and PR identity, Thread or Subject identity, file path and position, document fragment, or external URL. Use that target for click, context menu, clipboard, keyboard activation, and modifier policy. Keep display labels separate from addresses.

For a PR badge, choose native PR detail as the primary in-app destination and expose explicit integrated-browser and system-browser actions. If the product retains a different default, the label and visible result must make that choice clear. An explicit user open activates the required owner and shows its destination. Background updates retain their current non-stealing reveal protection.

Acceptance clicks B's PR while A is focused and the panel is hidden. B's intended PR becomes visible. Copy returns B's forge URL. Right-clicking the rest of B's row still offers its Thread link under an unambiguous label. Repeat issue references, file chips with line numbers, ordinary URLs, and fragments through pointer, menu, and keyboard paths. Missing target data produces an unavailable reason. No internal renderer scheme reaches the OS opener.

Sources are [pr-navigation.md](reports/pr-navigation.md), [link-affordances.md](reports/link-affordances.md), and `crates/ferrite-cockpit/src/cockpit/surfaces.rs:145-165`. Preserve streaming-selection click protections while consolidating policy.

### Unit 06. Complete focused-area navigation and semantic controls

After Unit 01 establishes ownership, route Close and Find through that owner. Supply named focusable controls for panel tabs, close, add, maximize, PR segments, links, citation actions, and previews. Use a consistent tab pattern and a documented area-navigation shortcut. Preserve the last valid control per area and return focus when a menu or overlay closes.

Acceptance operates the panel, PR segments, a file search, and a link menu without a pointer. Closing a panel tool never parks its parent Thread. File Find matches a sentinel absent from the transcript. Background redraws preserve focused controls. Hidden or closed tools cannot retain focus. Accessibility inspection reports names, roles, selection, disabled state, and focused state. Pointer close and reorder still work.

Unit 06 must not substitute a generic button whose default disables tab stops. Sources are `crates/ferrite-ui/src/components.rs:29-39,144-158`, `crates/ferrite-cockpit/src/cockpit.rs:6144-6164,9148-9175`, and the detailed input and link reports.

## Preserve content through authoring, send, and replay

### Unit 07. Persist structured draft and context records

Introduce versioned draft records keyed by DraftId before first send and by Thread afterward. Retain literal text, citation identity and range, user comments, review context, annotations, attachment identities, and dispatch intent separately. Promote a new draft to its Thread atomically. Keep legacy string decoding at a bounded import boundary and write the new representation after migration.

Acceptance round-trips empty and nonempty prose, literal leading blockquotes, several quotes, multiline comments, files, images, annotations, and review notes through save, restart, recall, queue editing, and definite send failure. Literal Markdown remains editable literal text. Context retains its controls and provenance. Adding a file cannot change a quote's rendering. No transport trailer becomes comment prose. New unsent panes restore their provider and checkout binding.

Keep provider-specific outbound formatting at the adapter. This unit should remove repeated split/recompose rules rather than add another decoder. Sources are `crates/ferrite-pane/src/composer.rs:494-554`, `crates/ferrite-pane/src/quote.rs:50-118`, `crates/ferrite-cockpit/src/cockpit/drafts.rs:7-64`, and composer-02, composer-11, quote-attachments-005 through -007.

### Unit 08. Make citing a complete editing journey

Build on Unit 07. Selection creates a source-aware citation and offers a reachable multiline comment editor. Support Save, Cancel, later Edit, remove, full quote inspection, and return to source. General message text remains independently editable. Whole-message attachment must either succeed or explain the size limit with an actionable alternative.

Replace the unreadable square prose card with a text-sized row or chip that can disclose the complete quote and comment. Keep images visually distinct. Bound the attachment region at narrow widths so the main editor and Send remain reachable. The collapsed state must show how many items it contains.

Acceptance selects actual rendered text, invokes Cite with pointer and keyboard, writes a comment, adds general instructions and a second citation, sends, then opens both sources from history. Save and Cancel preserve their respective text. Long and nearly identical quotes remain distinguishable. Restart preserves comments and source identities. Missing source history shows the retained quote and an explicit unavailable-source state. A long whole-message action never silently disappears.

Sources are `crates/ferrite-pane/src/transcript/reply.rs:59-95,206-224`, `crates/ferrite-pane/src/attachments.rs:505-618`, and [quote-attachments.md](reports/quote-attachments.md). Test-injected `Reply.text` is not acceptance evidence for authoring.

### Unit 09. Make attachment readiness and retained content truthful

Distinguish live workspace references from retained attachment snapshots. Store clipboard media durably under application ownership. Track destination environment, provider compatibility, preparation, transfer, failure, and retry. Ordinary local attachments to remote Threads must transfer or visibly block admission. Image decode success must not depend on a narrower optional dimension parser.

Acceptance attaches valid formats supported by the renderer, a missing image later repaired at the same path, a provider-unsupported image, and a local file to a remote fixture. Previews reflect decoder state and recover after repair. Provider payloads contain the expected remote content. Retry of a retained snapshot uses the original bytes after the source file changes. A live reference reports changed or missing content honestly. Keyboard preview and removal work in draft and sent history.

Show persistent error and retry within the affected attachment, with stable layout during preparation. This unit changes the historical path-storage tradeoff, so document migration and retention behavior. Sources are `docs/adr/0004-prompt-attachments.md:23-36`, `crates/ferrite-pane/src/pane.rs:3861-3955`, `crates/ferrite-pane/src/composer.rs:572-619,2104-2124`, and composer-03/-07 and quote-attachments-008/-010.

## Complete domain behavior and recovery

### Unit 10. Make PR reads target-specific, fresh, and recoverable

Use the canonical PR target from Unit 05 for snapshot keys and reads. Track request generation, cancellation, timeout, last good result, refresh state, and errors. Refresh must update detail, checks, activity, and guide as appropriate. Reject stale completions after target changes or newer reads. Preserve the last good content during a failed refresh.

Acceptance opens two repositories with the same PR number, switches targets during delayed reads, and completes responses out of order. Each tab shows only its target. Refresh updates a check or comment without requiring a PR tone change. A timeout, rate limit, missing credentials, and temporary network failure produce distinct recovery paths. Retry visibly issues a fresh request. Closing the tab prevents an old completion from changing selection or resurrecting it.

Sources are `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:274-335`, `crates/ferrite-reviewr/src/ferrite.rs:649-664`, and pr-operations-001 through -003 and -015/-016. Reuse the Changes generation pattern instead of inventing an app-wide lock.

### Unit 11. Complete the PR read and action journeys

The standalone PR browsing destination is required scope for this audit, following the user's explicit complaint that the PRs view is inaccessible. It must open from the global affordance even with an empty board or a focused thread whose branch has no PR. Keep project/list scope and explicit PR selection independent of the current checkout, and retain list state when returning from detail. This corrects the earlier placement of the global inbox among optional capabilities; `pr-navigation-03` remains the same source-proven finding.

Deliver this as separately verified subunits after Unit 10. First preserve the backend's full review structure, verdicts, replies, resolution, code context, ordering, and truncation indicators. Render Markdown through the supported rich path. Open a failed check's actual details. Second, give current agent-backed operations an explicit target and pending/result/error lifecycle with post-operation invalidation. Determine create availability from branch state rather than the current transcript's changed-file list.

Acceptance uses a PR fixture with nested replies, an empty-body approval, a resolved thread, more results than a server cap, two generated guides, and a failed check. The newest intended guide appears. Every hidden or capped part has an explicit retrieval or external route. A PR action remains associated with its target through queueing and completion. Failure clears pending state, retains useful content, and offers a valid retry. Merge availability and selected method reflect known capabilities or explain what remains unknown.

Native forge comments, reviews and metadata editing remain separate capability decisions. The standalone PR destination is required above. Existing visible actions must be trustworthy even if agent delegation remains the product model. Sources are `crates/ferrite-core/src/protocol/review.rs:478-505,575-610`, `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:159-205,689-840`, and [pr-operations.md](reports/pr-operations.md).

### Unit 12. Make the child conversation the primary Agent destination

Opening an agent should navigate the main conversation surface to that child, with immediate-parent and root navigation, full reading width and preserved parent draft, selected tool and scroll position. The earlier recommendation assumed the compact right Panel should remain the normal destination; that did not reflect T3's observed full child-conversation route. A side-by-side inspector can remain an explicit secondary action. This is a presentation/navigation correction, not a requirement to turn every Ferrite Subject into a top-level Thread. [Placement correction and evidence](reports/subagent-placement-correction.md).

Share Subject history loading, ancestry, coverage, decision ownership, supported transcript events, and stop targeting between compact and full views. Do not duplicate provider discovery. Make missing or partial history explicit, and show immediate-parent navigation. Derive Background inventories and stop targets from a common domain query so each view explains the same work.

Acceptance opens a live child, an evicted child, a nested child, and a child awaiting a decision from the rendered parent row. History hydrates without resuming the provider. Links and nested-agent actions work. A decision and Stop reach the correct child. A failed background stop clears its pending indicator and offers retry. General Thread navigation opens Main unless the user selected a specific notice or decision target.

Do not claim every live child is broken. The missing-history case requires an unretained child. Sources are `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:80-143`, `crates/ferrite-cockpit/src/cockpit/subagents.rs:376-438`, `crates/ferrite-core/src/engine/history.rs:110-172`, and [subagents-background.md](reports/subagents-background.md).

### Unit 13. Align close, reopen, and restart with resource lifetimes

Define close, hide, release, reopen, and restart behavior for each `SurfaceId`. Each reopenable kind must provide a way to recover its resource or show a nonblank failure state. Preserve the Files working set and relevant tool preferences, not only its outer tab. Restore selected inspection identity explicitly or explain its unavailability instead of silently choosing unrelated Background content.

Acceptance closes and reopens Agent and SideChat tabs containing unique markers. The same durable subject or conversation returns, or the product deliberately makes the destructive action non-reopenable and labels it clearly. No new fork is silently substituted. Restart restores eligible file tabs and selection. Missing files and unavailable agents show a recovery action. Terminal and browser lifecycle choices remain explicit and do not claim process restoration that never occurred.

File previews already hydrate and must retain that behavior. Sources are `crates/ferrite-cockpit/src/cockpit/surfaces.rs:48-53,342-405`, `crates/ferrite-cockpit/src/cockpit/panel.rs:124-141,314-319`, `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:298-319`, and [reconciliation.md](reports/reconciliation.md).

### Unit 14. Make the embedded browser usable as a website host

Separate external-page policy from sealed visualization policy. Complete new-window handling, origin-labeled dialogs, useful context actions, an explicit system-browser escape, and an address editor that preserves unfinished input. Model new-tab entry independently so navigation controls cannot act on a hidden old page. Preserve stable chrome geometry through loading and failure. Keep browser service ownership, manual intervention protection, and profile isolation.

Ship the native composition bridge as a separately checked part of this unit. Raw per-key character forwarding does not satisfy IME composition or caret placement. Use actual operating-system input methods on each supported platform.

Acceptance in a CEF build covers same-tab links, new-window links, scripted popup callback, alert, confirm accept/cancel, prompt text, address edits during page navigation, new-tab Cancel, Back/Forward, reload, page failure and retry, and system-browser open. Copy, Paste, Tab, Enter, and Escape must preserve the Unit 01 contract. Compose and cancel non-Latin input without a host command firing. Restored and reclaimed pages must disclose any intentional loss of page state.

Sources are `crates/ferrite-pane/src/visual/web/cef/handlers.rs:384-450,643-661`, `crates/ferrite-pane/src/visual/web/mod.rs:121-162`, and [browser.md](reports/browser.md). Responsive emulation, recording, profile import, and DevTools breadth remain separate options.

### Unit 15. Make Files, Changes, and terminal results complete and actionable

Use separately reviewable subunits for each tool. Refresh Files against workspace changes, disclose discovery limits, keep all inner tabs reachable, and route supported nontext files to their correct preview. Make source-position navigation and editing mode explicit for prose. Changes must expose the entire loaded diff, support horizontal access when wrapping is disabled, and make whitespace filtering truthful. Scope reviewed state to the actual content version.

Resolve the competing Files workspace/inner-tab ownership before adding more tab controls: identity, selection, close/reopen and restoration must agree. Assess terminal coexistence with files/browser explicitly; T3's bottom drawer and right-panel terminal are separate hosts. Adopting a bottom drawer remains a composition decision, while preserving correct focus and tool state is required either way. [Exact surface contracts](reports/surface-reconciliation-workspace.md).

Acceptance modifies the scratch checkout through a second actor and sees current tree and diff contents after the documented refresh. A file longer than the old visible diff cap remains fully reviewable. A long line is reachable. A reviewed file changed afterward becomes unreviewed or visibly stale. Stage/unstage failures and terminal spawn failures show target-specific errors and working retry without a false success state. Terminal URL and path actions preserve destination identity if adopted as part of its existing output interaction contract.

Sources are `crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:271-425`, `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:260-394`, and [panel-tools.md](reports/panel-tools.md). Terminal splitting is not a prerequisite.

## Repair scroll ownership before adding broader motion

### Unit 16. Give one operation ownership of transcript positioning

Represent reader navigation, live follow, requested jump, Find reveal, disclosure reveal, and restoration as coordinated viewport modes. Manual wheel, scrollbar track, thumb drag, and page keys revoke pending send anchoring and automatic movement regardless of whether a tween currently exists. Scope delayed work to transcript generation and stable RowId. Route End and the latest button through the same state transition, with timing as a separate choice.

Acceptance interrupts a send and a latest jump through each real gesture, then delivers later text and a tool event. The reader's chosen passage stays put. Repeat a thumb drag after the tween finishes while send runway remains. Find inside a tall visible row owns the result after the old tween expires and later content arrives. Expanding completed work preserves the clicked heading. A disappearing target cancels cleanly. Switching away and back never revives stale positioning from a different history generation.

Sources are [scroll-quality.md](reports/scroll-quality.md), `crates/ferrite-pane/src/transcript.rs:1033-1070,2323-2331`, and `crates/ferrite-pane/src/transcript/scroll.rs:163-169,316-324`. Reuse the native list's measurement rebasing and drag freeze. Do not add full-history measurement.

### Unit 17. Make scroll geometry and reading continuity testable

After Unit 16, unify thumb painting, drag, and track-click coordinate conversion. Distinguish real content extent from synthetic bottom room. Compute page travel from the unobscured viewport. Preserve a suitable local reading anchor through reflow, including a passage inside a tall row. Specify a deliberate long-distance jump policy compatible with virtualization.

Acceptance compares track clicks with their painted thumb positions at several fractions and at the minimum thumb size. A short answer with remaining runway still shows the latest cue when real content lies below the viewport. End and the pill land at the same final row without extra runway. Numbered lines under a pinned prompt remain continuously readable through Page Down and Page Up. A distinctive sentence stays at its readable inset while the panel opens, closes, maximizes, and resizes.

Record intermediate frames for long and unmeasured jumps. A final offset assertion cannot establish continuity. Keep tall-row and concurrent-index risks labeled inferred until their fixtures reproduce them. Sources are `crates/ferrite-ui/src/scrollbar.rs:544-575,860-903,959-978`, `crates/ferrite-pane/src/transcript/scroll.rs:128-133,238-297`, and scroll-quality-002/-003/-007/-008/-009.

### Unit 18. Define and implement the visual and motion contract

Build on stable input, geometry, and scroll ownership. Use Ferrite's `Tween`, `Reveal`, retained exits, and existing tokens. Define entrance, exit, interruption, focus, and reduced-motion behavior for each transition below. Sidebar and panel open/close motion are explicit requirements from the user. T3's zero default does not decide Ferrite's product policy.

| Transition | Required observable behavior |
| --- | --- |
| Sidebar and panel open/close | Intermediate geometry is visible, reversal starts from the current frame, and hidden controls cannot receive input. The reading anchor survives reflow. |
| Panel tab switch | The selected tab and body agree throughout. Outgoing content cannot accept actions after ownership changes. Focus transfers or restores deliberately. |
| Resize and maximize | Drag remains directly attached to the pointer. Programmatic size changes follow a specified policy without persisting a tool's minimum width as the user's permanent preference. |
| Browser load/error/retry | Address and page bounds remain stable. Progress and failure states occupy reserved chrome and preserve the user's address draft. |
| Citation and attachment add/remove | Text remains readable. Removal closes the occupied space predictably and places focus on a valid neighbor or editor. Preparation does not repeatedly move Send. |
| Menu, shelf, and disclosure | Opening and closing preserve the origin and readable anchor. Rapid reversal produces no duplicate or invisible interactive content. |
| Send and live follow | The sent prompt, answer reveal, and first tool follow the viewport owner. Late provider events cannot override manual navigation. |
| Reduced motion | Motion reaches the same valid state with no continuous animation or delayed ownership. Live OS preference changes apply without restart on supported platforms. |

Acceptance records before, intermediate, and final frames at narrow, ordinary, and wide window sizes. Also exercise rapid reversal, close during entry, background updates during motion, and reduced-motion changes. Inspect focus and hit testing while recording. No frame-rate claim is allowed without a separate measurement method. Validate the browser and citation layouts with real content, including long titles, multiline comments, missing assets, and errors.

Sources are `crates/ferrite-ui/src/motion.rs:923-1042,1335-1365,2152-2177`, `crates/ferrite-ui/src/theme.rs:3531-3574`, [motion.md](reports/motion.md), and [quote-attachments.md](reports/quote-attachments.md).

## Finish environment, navigation, and transcript recovery

### Unit 19. Target settings and provider management to the right environment

Separate local presentation preferences, remote server defaults, project defaults where supported, and effective running-session facts. Send management operations to an explicit environment. State which machine a version probe or update affects. Recover unavailable native sessions through a defined same-Thread continuation or an explicit user choice that preserves history and unsent work.

Acceptance connects to a temporary remote fixture with different local and remote values. Editing a server default changes and rereads the remote value. Local UI preferences remain local. An update reports the binary it targeted and invalidates only relevant sessions. Approval cards reflect the provider-announced sandbox. A missing native resume target offers a usable recovery path without looping through the same failure or discarding the draft.

Sources are [providers-settings.md](reports/providers-settings.md), especially PS-01 through PS-05. Additional providers, named instances, and relay connectivity are independent product extensions.

### Unit 20. Repair navigation and transcript completeness in bounded units

First, make offered navigation actions work on every row representation. Parked and Snoozed Rename must render the editor. Search selection must track a stable Thread identity, remain visible while moving, and survive live updates. Reopen and bulk Park must report individual failures. Undo must restore the membership state it claims to undo. Opening an out-of-scope Thread must reveal or explain its sidebar location.

Second, separate transcript search coverage from disclosure rendering. A match in folded content must be discoverable and reveal the right occurrence under the Unit 16 viewport policy. Retain readable completed plan state. Keep work-group expansion bounded even when a nested tool is open. Confirm the long-history retrieval gap with a fixture crossing retention before selecting an incremental paging repair.

Acceptance performs the rendered Rename, search, Park, Undo, and failed reopen flows with updating fixtures. Search never opens a different Thread merely because result order changed. Transcript Find reaches hidden work and retains its occurrence. Completed plan updates remain inspectable. Long histories either retrieve earlier content or clearly state the boundary with a working retrieval path. Restored window geometry remains usable after a monitor disappears.

Sources are [projects-navigation.md](reports/projects-navigation.md), [transcript.md](reports/transcript.md), and coverage-quality-05. Broader content search, multi-project scopes, and pin reordering may follow after the existing navigation contract is reliable.

## Optional T3 capabilities and explicit product choices

These capabilities deserve separate proposals with their own user value and acceptance criteria. Their presence in T3 does not make them prerequisites for the repairs above.

| Capability or choice | Dependency and scope |
| --- | --- |
| Native comments/reviews, metadata editing, richer review guide | Build on canonical PR targets, complete domain records, and reliable mutation state. Agent delegation can remain supported. The standalone PR inbox is required in Unit 11. |
| Queue versus steer choice, queue hold on Stop, transactional queue editing | Decide the dispatch contract explicitly. Current queue and Stop behavior is partly intentional. At minimum label it truthfully and preserve unrelated drafts. |
| Prompt stash, inline rich editing, voice input, multi-model first send | Build on durable structured drafts. Do not introduce another lossy content representation. |
| Additional providers and named instances | Build on environment-aware management and truthful effective capabilities. |
| Responsive browser emulation, DevTools breadth, recording/export, managed profiles | Build on the working browser input and lifecycle boundary. Keep sealed visualization rules separate. |
| Terminal splits and richer file-management actions | Build on reliable resource and file-save lifecycles. |
| Guest participation, device panels, one-time or sub-hour schedules | Scope independently. Ferrite already has SSH and recurring Factory automation. These additions are not repairs to absent whole systems. |
| Light appearance, font customization, richer Markdown media and math | Scope against user demand after readable layout, keyboard access, and truthful state are established. |

The comparison evidence and availability limits are in [coverage-quality.md](reports/coverage-quality.md), [composer.md](reports/composer.md), [providers-settings.md](reports/providers-settings.md), and the relevant tool reports. Do not add every capability to the essential program under a generic parity label.

## Evidence required to close a repair

Each implemented unit needs its own concise evidence packet. Include the original trigger, exact source/build identity, observed before/after state, the narrow green test command, the applicable typecheck, and the one relevant E2E run. Capture the visible result and the underlying side effect where they differ. A link test needs the destination and selected owner, a send test needs the provider request, and a save test needs the file and retained buffer.

For asynchronous work, include one delayed completion, one failure, and recovery. For interactive state, include focus before and after a redraw. For motion, include intermediate positions and interruption. For persistence, include a restart into the same identity. For content, compare the source, draft, provider payload, sent display, recall, and retry without silently changing semantics.

A green helper test cannot close an entry-point defect. An endpoint screenshot cannot close a scroll transition defect. A source-defined button cannot close a missing authoring journey. Optional capability breadth cannot compensate for failure in a currently offered workflow.
