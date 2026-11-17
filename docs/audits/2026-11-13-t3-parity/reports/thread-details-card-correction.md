# The floating Thread details card was not properly captured

The user means the box at the top right of T3's transcript. Its implementation is `ThreadDetailsPanel` inside `ThreadDetailsCard`. It is distinct from the tabbed right Workspace panel and the floating browser preview. The earlier audit named its toggle in the shell checklist and discussed some lineage content, but did not retain the card's independent presentation as a finding. That was an omission.

## Finding quality-correction-005

**P2 · partial capability · source-proven.** Ferrite places related thread information inside Overview, which occupies the same tabbed right Panel as Browser, Files, Changes and other tools. It lacks T3's separate compact details presentation. Related information existing inside Overview does not provide equivalent visibility or access while another tool occupies that panel.

This differs from `panel-tools-018`: that record covers automatic tool widths overwriting remembered user size. This record covers the missing separation between lightweight thread context and the full tool workspace. A corrected saved-width algorithm alone cannot supply an independently available details card.

Ferrite's [Overview branch](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L657-L670) renders its sections inside the active panel. [The section renderer](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/sections.rs#L1-L39) describes environment, context, connectors, changes and background work. This is a source-proven composition gap; the new native checks below concern T3, not a newly run Ferrite reproduction.

## Exact installed T3 implementation

All links in this section use installed commit `1264c206aee0dce805d6279809118bc091f29d2f`.

- [ChatView mounts the details panel](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3832) separately from the tool workspace.
- [ThreadDetailsCard renders an absolute upper-right aside](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsCard.tsx#L149). Its [layout calculation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/threadDetailsCardLayout.ts#L17) uses a 280 CSS-pixel card and 12-pixel edge gaps, leaves room for Find, and falls back to a popover when the remaining conversation width or height is insufficient. These are source values, not screenshot measurements.
- [Workspace controls](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsPanel.tsx#L166) contain execution context, editor opening and project scripts. [Version Control](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsPanel.tsx#L216) contains branch, Git actions and Changes. Later sections conditionally include servers, automations, lineage, side chats, usage and context.
- [Visibility state](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L211) is scoped by thread and presentation: inline defaults open, popover defaults closed. [The dedicated header toggle](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/PanelLayoutControls.tsx#L57) and [Command-Shift-B binding](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/shared/src/keybindings.ts#L68) are separate from the workspace panel. [The action handler](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useRightPanelActions.ts#L222) also manages focus.
- [Card context-menu handling](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsPanel.tsx#L132) exposes item customization. Layout chooses full, compact or essential density. No drag, resize or separate minimize operation was established in the card implementation; “floating” here describes anchored presentation, not a freely movable window.

## Focused native recheck

T3 was reopened to the existing empty disposable draft. No model was submitted and neither archived audit root was resumed.

1. A native screenshot showed the floating rounded details card at the top right, with workspace context, Open in Finder, Add project script, branch, Commit and Changes. The workspace-panel toggle was off.
2. Clicking Toggle thread details removed the card and changed its value from 1 to 0. The full right-panel toggle stayed 0.
3. Command-Shift-B restored the card and value 1, and focus moved to Run context.
4. Opening the full workspace panel showed its separate Open a surface launcher. Details changed to its closed popover presentation. Activating its own toggle showed the compact card beside the still-visible workspace launcher. A fresh screenshot confirmed both surfaces.
5. Escape dismissed the details popover, returned focus to its toggle and left the workspace open.
6. Closing the workspace restored the original inline details card automatically: details 1, workspace 0. This verifies the observed distinction between the two presentation states.
7. T3 was quit through its native app menu. A subsequent app inventory reported `isRunning=false`. No fixture server, provider task or model run was started during this recheck.

Screenshots were displayed in the conversation, not exported as saved PNG files. These checks establish identity, basic visibility, shortcut, focus, one responsive mode change and coexistence with the empty workspace launcher. They do not certify coexistence with every loaded tool, customization actions, Find clearance, full/compact/essential transitions, every narrow size, live content updates, or animation timing.

## Required repair and acceptance

Give thread context a lightweight, independently accessible presentation that preserves useful conversation space and remains available when a full tool view is open. Separate its visibility and sizing behavior from the workspace's active tab and remembered width. Make the relation to the selected thread explicit; the box must not show another thread's agents, branch, approvals or usage.

Verify pointer and shortcut open/close, focus return, Escape, content overflow, narrow-window fallback, coexistence with Browser/Files/PR, Find clearance, and thread switching. Verify each agent link opens the correct main child conversation and each Git/PR control opens the correct full tool destination. Include meaningful appearance/disappearance motion, interruption and reduced-motion behavior in the design and runtime checks. A generic Overview tab, a smaller forced panel width, or copied card styling alone does not satisfy this workflow.

The consolidated inventory adds this as `quality-correction-005`. Earlier 176-record and 273-checklist validation artifacts remain historical snapshots. The earlier Sonnet artifact review predates this correction; the focused source check used the existing source specialist.
