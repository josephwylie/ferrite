# Additional quality findings from the corrective pass

These five findings are distinct from the original source inventory and are included in the consolidated tally. They concern existing surfaces and their composition. The parent inspected the Agent width comparison, Browser hierarchy and native PR error evidence; the later floating-details finding combines a Ferrite source trace with a focused native T3 recheck. Successful output delivery does not establish that the result is readable or that its controls form a complete workflow.

## quality-correction-001 — Agent transcript prose clips at ordinary panel width

P1 · runtime-reproduced

At a 420px panel, child transcript sentences lose their endings beyond the right edge. Maximizing to 960px reveals the complete text. Output arrival alone does not make the agent screen readable.

Parent inspected both screenshots. Source references locate the rendering path; they are not a proven root cause or proposed patch.

Repair direction: Measure and wrap child prose against the actual panel content width; retain reading position across maximize, restore and resize. The precise layout cause remains to be isolated.

Acceptance: Open a child with ordinary multi-sentence output at minimum and ordinary widths. Every character remains readable without maximizing; resizing and restoring reflow existing output without waiting for another event.

Evidence:

- [agent-live-correction.md](../reports/agent-live-correction.md)
- [13-completed-agent.png](../evidence/agent-live-correction/13-completed-agent.png)
- [14-agent-maximized-output.png](../evidence/agent-live-correction/14-agent-maximized-output.png)

Source paths at the pinned commits:

- crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-225
- crates/ferrite-pane/src/transcript.rs:1527
- crates/ferrite-pane/src/transcript.rs:2955

## quality-correction-002 — Browser retains a duplicate heading, nested tabs and legacy card beneath shared panel tabs

P1 · runtime-reproduced

The Browser surface row is followed by a second Browser title, a page-tab strip and then the toolbar. Repeated identities, differently scoped plus controls and competing frames consume space and make the existing browser look unfinished.

Chrome observed in headless screenshots and personally in installed Ferrite with a loaded example.com page. The unavailable headless page body is expected and is not counted as a native browser-load defect. This is a visual-quality judgment supported by actual assembly and ADR 0039.

Repair direction: Complete the shared surface model: give each page one clear identity and unify tab/control density; remove the redundant legacy Browser heading/card layer where the panel already provides it.

Acceptance: Open one and two pages, switch to Files and back, and close the active page. Active identity and add/close scope remain clear, with a single deliberate panel hierarchy and aligned content edges.

Evidence:

- [visual-quality-correction.md](../reports/visual-quality-correction.md)
- [13-browser-after-files.png](../evidence/panel-correction/13-browser-after-files.png)
- [15-browser-after-changes.png](../evidence/panel-correction/15-browser-after-changes.png)

Source paths at the pinned commits:

- crates/ferrite-cockpit/src/cockpit/panel.rs:636-645
- crates/ferrite-cockpit/src/cockpit/panel/preview.rs:498-515
- crates/ferrite-cockpit/src/cockpit/browser.rs:1355-1405
- crates/ferrite-pane/src/preview.rs:641-672
- docs/adr/0039-the-panel-is-one-tabbed-surface-host.md:1-24
- apps/web/src/components/RightPanelTabs.tsx:599-641
- apps/web/src/components/preview/PreviewChromeRow.tsx:94-95

## quality-correction-003 — Empty browser address entry lacks a clear editable-region affordance

P2 · runtime-reproduced

The empty location row reads as navigation icons, a static Address label, blank space and a separate arrow. The editable region has no prompt; the label and annotation control also spend address width.

Only empty presentation is directly observed here. Native hover/focus/narrow-width behavior remains unverified. T3 also uses a transparent idle field, so absence of a permanent border is not itself the defect.

Repair direction: Design one location control with explicit empty, idle, hover, focus, editing, loading and invalid states while preserving useful one-line address space.

Acceptance: An empty tab clearly indicates where to enter a URL before interaction. Hover and keyboard focus identify the field; text and navigation remain usable at supported widths. Verify these native states individually.

Evidence:

- [visual-quality-correction.md](../reports/visual-quality-correction.md)
- [13-browser-after-files.png](../evidence/panel-correction/13-browser-after-files.png)

Source paths at the pinned commits:

- crates/ferrite-cockpit/src/cockpit/browser.rs:180-184
- crates/ferrite-cockpit/src/cockpit/browser.rs:1452-1488
- crates/ferrite-pane/src/composer.rs:1084-1136
- apps/web/src/components/preview/PreviewChromeRow.tsx:153-215
- apps/web/src/components/ui/input-group.tsx:8-27

## quality-correction-004 — Native PR read errors run beyond the panel and hide their cause and recovery text

P2 · runtime-reproduced

A long PR read error is centered on one unwrapped line wider than the panel. Its beginning and recovery instruction are clipped, leaving the operator unable to read the failure.

Parent inspected the screenshot and the centered pr_note render path. The nonexistent synthetic repository caused this expected read error; it is not evidence that normal repositories fail.

Repair direction: Render read failures in a width-constrained, wrapping body with a visible relevant recovery action. Preserve the current PR identity and useful previous content.

Acceptance: Force a long read error at ordinary/minimum panel widths. The full cause and next action remain readable and reachable; retry recovers without losing target identity.

Evidence:

- [pr-entry-runtime-correction.md](../reports/pr-entry-runtime-correction.md)
- [04-overview-native-pr.png](../evidence/pr-entry-correction/04-overview-native-pr.png)

Source paths at the pinned commits:

- crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:452-469
- crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:867-879

## Consolidation decisions

### Follow-up: quality-correction-005 — Thread context lacks a separate compact details surface

P2 · partial capability · source-proven. The top-right floating Thread details card in T3 was named in the checklist but omitted as a standalone composition gap. Ferrite's related context occupies Overview inside the full tabbed workspace panel. This is distinct from the shared-width persistence bug: context cannot remain independently available alongside another active tool merely by fixing saved width. [Exact installed source, focused native checks and acceptance](thread-details-card-correction.md).

The forced panel width remains `panel-tools-018`; blank Agent and SideChat reopen both remain `subagents-background-03`; the agent live-output route refines `subagents-background-08`. PR entry and stale refresh, folded Find, draft persistence, queued editing, file cache/tree/tab failures and quote inspection gain scoped evidence on their existing records. This avoids counting the same defect once for source and again for runtime.

The screenshot-only paging blankness is inconclusive because unrelated window chrome also disappears. The elicitation form input path is inconclusive because recorded clicks targeted parent wrappers rather than a verified nested input. Neither becomes a new product defect. Browser-host unavailability in the no-CEF test build is expected.

The missing sidebar/panel animation remains confirmed by the explicit snap implementation and the user requirement. A settled image does not measure motion timing; no still image is presented as such proof.
