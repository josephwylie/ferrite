# Correcting the surface-level audit

The earlier mistake was identifying the user's “floating box at the top right of the transcript” as a browser preview. It is T3's **Thread details card**. This was an interpretation error by the auditor, not a newly discovered defect in T3's browser preview. The underlying audit failure was treating feature names and component existence as adequate coverage of placement, interaction and lifetime. The user had already asked for that broader investigation and should not have needed to identify these omissions.

This follow-up starts at the installed application's rendered roots and gives each major destination, conditional overlay and persistent host an explicit identity. Six source workers divide shell, conversation, browser, workspace, independent completeness and prior-coverage reconciliation. The parent alone operates the native app. Installed evidence is pinned to `1264c206aee0dce805d6279809118bc091f29d2f`; the newer comparison checkout remains separate.

## What changes in the repair requirements

| Surface distinction | What the earlier audit missed or weakened | Correct requirement and evidence |
|---|---|---|
| Main child conversation versus compact inspector | The repair proposal kept the right-panel Agent as the normal destination despite T3 opening a full child conversation. | Make the child the primary main destination; preserve the parent draft, tool selection and reading position when returning. A compact inspector can be secondary. [Placement correction](subagent-placement-correction.md), [conversation map](surface-reconciliation-conversation.md). |
| Thread details versus full workspace | Naming the card in a checklist did not capture independently available thread context. Ferrite Overview occupies the large tool workspace. | Independently show compact context alongside Browser/Files/PR; retain separate visibility, responsive behavior, focus and dismissal. This is the existing added finding `quality-correction-005`, not another new finding. [Details correction](thread-details-card-correction.md). |
| Standalone PR destination versus linked PR detail | The global PR destination was incorrectly left among optional capabilities. | A required global entry must work without a linked PR or selected PR-bearing branch; list scope and selected PR remain independent of the current checkout. Existing `pr-navigation-03` moves to required repair scope. [Unit 11](../remediation.md#unit-11-complete-the-pr-read-and-action-journeys). |
| Docked browser, in-app mini-player and separate native preview | “Floating preview” was too broad and led to the wrong referent. | Keep the interactive mini-player, docked browser, always-on-top native image mirror and detached DevTools distinct. Native local page dialog limitations do not establish the streamed browser's limitations. [Browser presentation map](surface-reconciliation-browser.md). |
| Bottom terminal drawer versus right-panel terminal | A Terminal tab does not describe simultaneous terminal and file/browser access. | T3 has independent hosts. Record Ferrite's current right-panel ownership and assess simultaneous-use needs explicitly. A second terminal placement remains a composition decision, not an automatically reproduced defect. [Workspace map](surface-reconciliation-workspace.md). |
| Files resource tabs versus nested inner tabs | Existing overflow findings did not fully articulate the competing tab ownership. | Give file identity, selection, close/reopen and restoration one coherent model; avoid requiring users to manage two unrelated tab layers. Existing clipping, stale-file and restoration findings remain distinct. [Workspace map](surface-reconciliation-workspace.md). |
| Main child, Side chat, child approvals and provider-specific dialog | A single “subagent UI” label obscures different jobs. | Main child navigation is not embedded Side chat. Related-child approvals above the composer are not historical approval rows. The Antigravity-only child-work dialog is not the Sol/Sonnet child route. [Conversation map](surface-reconciliation-conversation.md). |

## Fresh native checks

The already-completed Sol audit root was temporarily unarchived. No new model turn was submitted. A new unsent sentinel draft made preservation observable.

1. Opened Files and README.md; the sentinel stayed intact.
2. Opened Thread details beside that loaded README. Screenshot inspection showed the compact floating card over the transcript and the source/explorer in the separate right workspace.
3. Opened the completed Sol Output Probe through Previous agents. The main route and full transcript changed to the child; the parent's sidebar row retained an unsent-draft marker. The child displayed its completed state and Open parent.
4. Open parent restored the exact draft and selected README; focus returned to the composer. The transient details popover was closed.
5. Opened the bottom terminal drawer. Screenshot inspection showed terminal, transcript/composer and right-hand README together. No terminal command was entered.
6. Closed the empty shell with its confirmation, cleared the sentinel, closed README, restored inline details, rearchived the Sol root and quit T3. Fresh native inventory reported `isRunning: false`.

The screenshot evidence for these T3 checks is in the tool conversation, not newly saved app-chrome PNGs. [Observation ledger](../evidence/surface-reconciliation/observations.md), [cleanup record](../evidence/surface-reconciliation/cleanup.json). These checks do not retest streaming, certify scroll restoration, establish all narrow-screen behavior or prove Ferrite works.

## Evidence and completeness

The [independent rendered-root inventory](surface-reconciliation-completeness.md) is the canonical surface manifest. The [coverage reconciliation](surface-reconciliation-gaps.md) maps each canonical entry to existing findings and evidence, including cases previously only mentioned in a checklist. Specialist maps cover [shell](surface-reconciliation-shell.md), [conversation](surface-reconciliation-conversation.md), [browser](surface-reconciliation-browser.md) and [workspace](surface-reconciliation-workspace.md).

This inventory includes conditional and nonvisual hosts; its entry count is not a feature count, defect count or number of completed tests. The existing 273-case T3 snapshot and 52-journey Ferrite matrix remain historical, bounded evidence. New observations are kept separately so their old fingerprints remain meaningful.

The consolidated inventory remains **177 findings**. The scope correction yields **138 required repair/quality records and 27 capability candidates**, with six product choices and six verification gaps. Source and native outcomes remain separately labeled. The application source is unchanged.

The earlier Sonnet artifact review predates this follow-up and did not review the original tool transcript or screenshot pixels. This follow-up's independent source reconciliation is not a cross-family visual sign-off.

The separate Sol source reviewer checked the final placement and lifetime distinctions and found two stale evidence statements: the conversation map still described only empty-launcher coexistence, and the workspace map omitted the fresh drawer check. Both MD/JSON pairs now include the loaded-README and drawer observations, with commands, splits, motion and untested width variants still excluded. The reviewer performed no additional native actions or model runs.
