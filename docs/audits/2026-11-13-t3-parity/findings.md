# Consolidated finding inventory

The user wants a production-quality audit of Ferrite against T3 Code. This inventory records complete behavior gaps, not just feature presence. Priorities and merges follow [adjudications.json](adjudications.json). Source proof is not a claim of runtime reproduction. Capability candidates require a product decision and are not automatically Ferrite defects.

There are 177 consolidated records from 181 original records across 17 investigation partitions.

| ID | Priority | Evidence | Category | Finding |
|---|---|---|---|---|
| [BROWSER-01](#browser-01) | P1 | source-proven | defect | New-window links and scripted popups are silently discarded |
| [BROWSER-02](#browser-02) | P1 | source-proven | defect | Website JavaScript dialogs are always dismissed |
| [composer-01](#composer-01) | P1 | source-proven | defect | An idle send can clear the only draft before admission fails |
| [composer-02](#composer-02) | P1 | runtime-reproduced | defect | A new unsent pane has no durable draft record |
| [composer-03](#composer-03) | P1 | source-proven | defect | Ordinary local attachments are not transferred to a remote machine |
| [coverage-quality-01](#coverage-quality-01) | P1 | source-proven | verification-gap | UI interaction tests do not gate ordinary PR CI |
| [input-focus-001](#input-focus-001) | P1 | source-proven | defect | Host action bindings take browser and empty-panel input before their local handlers |
| [input-focus-003](#input-focus-003) | P1 | source-proven | interaction | Right-panel tabs, close buttons and PR controls have no keyboard focus or accessible control roles |
| [input-focus-004](#input-focus-004) | P1 | source-proven | interaction | Cmd+W on a PR or terminal can close the parent conversation instead of the active panel surface |
| [input-focus-007](#input-focus-007) | P1 | source-proven | interaction | Typing y, n or a as the first character of an empty approval composer performs a permission action |
| [input-focus-010](#input-focus-010) | P1 | runtime-reproduced | defect | Files editor and filter lose keyboard focus when their edits redraw the cockpit |
| [panel-tools-001](#panel-tools-001) | P1 | source-proven | defect | A failed file save is silent and closing the file discards the edited buffer |
| [panel-tools-002](#panel-tools-002) | P1 | source-proven | defect | Cached editor buffers can overwrite newer agent or other-thread changes |
| [panel-tools-003](#panel-tools-003) | P1 | source-proven | defect | The file tree never refreshes cached directory entries or Git status |
| [panel-tools-005](#panel-tools-005) | P1 | runtime-reproduced | interaction | Nested file tabs overflow into an inaccessible clipped region |
| [panel-tools-009](#panel-tools-009) | P1 | source-proven | defect | Changes cuts each file at 120 displayed lines with no expansion action |
| [panel-tools-015](#panel-tools-015) | P1 | source-proven | defect | Terminal startup failures leave no visible result |
| [pr-navigation-01](#pr-navigation-01) | P1 | runtime-reproduced | defect | Clicking another thread's PR routes to a panel that is not shown |
| [pr-operations-001](#pr-operations-001) | P1 | source-proven | defect | PR detail stays stale after status refresh and agent actions |
| [pr-operations-020](#pr-operations-020) | P1 | source-proven | defect | Create PR is unavailable for branch work absent from the current thread transcript |
| [projects-navigation-001](#projects-navigation-001) | P1 | source-proven | defect | Rename on Parked and Snoozed rows creates an editor that is never drawn |
| [PS-01](#ps-01) | P1 | source-proven | defect | Approval cards can state the wrong Codex sandbox |
| [PS-02](#ps-02) | P1 | source-proven | defect | Remote permission and server defaults save to the local machine |
| [quality-correction-001](#quality-correction-001) | P1 | runtime-reproduced | defect | Agent transcript prose clips at ordinary panel width |
| [quality-correction-002](#quality-correction-002) | P1 | runtime-reproduced | interaction | Browser retains a duplicate heading, nested tabs and legacy card beneath shared panel tabs |
| [quote-attachments-001](#quote-attachments-001) | P1 | runtime-reproduced | missing-capability | Cite has no reachable per-quote comment editor |
| [quote-attachments-007](#quote-attachments-007) | P1 | source-proven | defect | Quoted file and skill syntax can become active provider input |
| [quote-attachments-012](#quote-attachments-012) | P1 | unverified | verification-gap | The reported pointer Cite regression still needs installed-app reproduction |
| [subagents-background-02](#subagents-background-02) | P1 | source-proven | partial-capability | Nested agents and immediate-parent navigation are missing from the normal drill path |
| [subagents-background-03](#subagents-background-03) | P1 | runtime-reproduced | defect | Reopen closed tab restores empty Agent and SideChat tabs |
| [BROWSER-03](#browser-03) | P2 | source-proven | interaction | Right-click has no browser context menu |
| [BROWSER-04](#browser-04) | P2 | source-proven | interaction | Browser tabs discard page titles and show only hostnames |
| [BROWSER-05](#browser-05) | P2 | source-proven | partial-capability | Address entry rejects normal schemeless hostnames |
| [BROWSER-06](#browser-06) | P2 | source-proven | defect | Page navigation can overwrite an unfinished address edit |
| [BROWSER-07](#browser-07) | P2 | source-proven | defect | New-tab mode leaves navigation controls pointed at the hidden old page |
| [BROWSER-08](#browser-08) | P2 | source-proven | interaction | Loading changes browser geometry and shifts the address field |
| [BROWSER-09](#browser-09) | P2 | source-proven | missing-capability | Viewport size is inseparable from panel size |
| [BROWSER-10](#browser-10) | P2 | source-proven | missing-capability | Find, DevTools, hard reload, and page appearance controls are absent |
| [BROWSER-11](#browser-11) | P2 | source-proven | partial-capability | Inactive-page reclamation restores only a URL, losing navigation and page position |
| [BROWSER-12](#browser-12) | P2 | source-proven | missing-capability | An empty browser provides neither useful guidance nor recent/local destinations |
| [BROWSER-13](#browser-13) | P2 | source-proven | partial-capability | Persistent website profiles have no operator management controls |
| [BROWSER-14](#browser-14) | P2 | source-proven | missing-capability | Browser capture is annotation-oriented and lacks direct recording/export affordances |
| [BROWSER-16](#browser-16) | P2 | source-proven | missing-capability | No browser-chrome escape to the system browser |
| [composer-04](#composer-04) | P2 | runtime-reproduced | partial-capability | Queued Edit removes the message and merges it into unrelated draft text |
| [composer-05](#composer-05) | P2 | source-proven | missing-capability | The composer cannot choose queue versus steer per message |
| [composer-06](#composer-06) | P2 | source-proven | intentional-difference | Stop deliberately allows queued work to continue |
| [composer-07](#composer-07) | P2 | source-proven | partial-capability | Attachment readiness and native-image fallbacks are invisible |
| [composer-08](#composer-08) | P2 | source-proven | missing-capability | Large pastes stay inline with no composer length guard |
| [composer-09](#composer-09) | P2 | source-proven | partial-capability | Context is mostly a shelf or serialized trailer rather than inline records |
| [composer-10](#composer-10) | P2 | source-proven | missing-capability | The mention menu has no PR, thread or model-reference search |
| [composer-11](#composer-11) | P2 | source-proven | defect | Review comment drafts lose their structured controls after reopen |
| [coverage-quality-02](#coverage-quality-02) | P2 | source-proven | verification-gap | Some UI tests bypass real keyboard and opener dispatch |
| [coverage-quality-03](#coverage-quality-03) | P2 | source-proven | verification-gap | Required-form editability coverage never proves valid typed submission |
| [coverage-quality-04](#coverage-quality-04) | P2 | source-proven | verification-gap | Default UI checks do not establish native Chromium behavior |
| [coverage-quality-05](#coverage-quality-05) | P2 | source-proven | interaction | Window geometry still resets at launch |
| [coverage-quality-09](#coverage-quality-09) | P2 | runtime-reproduced | interaction | CLI help advertises variadic provider arguments that the parser rejects |
| [input-focus-005](#input-focus-005) | P2 | source-proven | missing-capability | No workspace-area focus cycle or per-surface focus restoration connects the composer and panel |
| [input-focus-006](#input-focus-006) | P2 | source-proven | partial-capability | Embedded browser input has no composition or native text-input bridge |
| [input-focus-008](#input-focus-008) | P2 | inferred | interaction | Panel resizing relies on root hover events that occluding children can block |
| [input-focus-009](#input-focus-009) | P2 | runtime-reproduced | defect | Find in a Files editor searches the conversation instead of the file |
| [link-affordances-01](#link-affordances-01) | P2 | runtime-reproduced | missing-capability | Transcript web-link menus cannot explicitly open the system browser |
| [link-affordances-02](#link-affordances-02) | P2 | source-proven | interaction | Modified and middle clicks on transcript web links ignore destination intent |
| [link-affordances-03](#link-affordances-03) | P2 | source-proven | defect | Issue-reference context menus open and copy an internal scheme instead of the real URL |
| [link-affordances-04](#link-affordances-04) | P2 | runtime-reproduced | partial-capability | Custom file chips lose target-aware context actions |
| [link-affordances-05](#link-affordances-05) | P2 | source-proven | defect | Markdown section links are sent to the OS instead of scrolling within the document |
| [link-affordances-06](#link-affordances-06) | P2 | source-proven | interaction | Ordinary Markdown links and their transcript menus have no keyboard activation path |
| [link-affordances-07](#link-affordances-07) | P2 | source-proven | defect | Right-clicking a sidebar PR badge can offer Copy link for the thread instead of the PR |
| [motion-001](#motion-001) | P2 | source-proven | motion | Sidebar and right-panel geometry snap across structural changes |
| [motion-003](#motion-003) | P2 | source-proven | motion | Most panel tab bodies have no transition despite the motion catalog |
| [motion-005](#motion-005) | P2 | source-proven | motion | Shelves and connector details reveal their bodies instantly |
| [motion-006](#motion-006) | P2 | source-proven | defect | System reduced-motion changes are not observed while Ferrite runs |
| [motion-007](#motion-007) | P2 | source-proven | partial-capability | Non-macOS builds do not read the system reduced-motion preference |
| [motion-008](#motion-008) | P2 | source-proven | motion | Streaming tail follow still jumps with content growth |
| [panel-tools-004](#panel-tools-004) | P2 | source-proven | partial-capability | File discovery silently omits entries beyond fixed limits |
| [panel-tools-006](#panel-tools-006) | P2 | source-proven | partial-capability | Restart restores the Files shell but loses its open files and tool preferences |
| [panel-tools-007](#panel-tools-007) | P2 | source-proven | partial-capability | Markdown and prose files cannot be edited or navigated to a requested source line in Files |
| [panel-tools-008](#panel-tools-008) | P2 | source-proven | partial-capability | Opening image or document files from the tree bypasses the available preview path |
| [panel-tools-010](#panel-tools-010) | P2 | source-proven | defect | Disabling diff wrapping clips long lines instead of allowing horizontal scrolling |
| [panel-tools-011](#panel-tools-011) | P2 | source-proven | defect | Hide whitespace changes only removes blank changed lines |
| [panel-tools-012](#panel-tools-012) | P2 | source-proven | defect | Changes stays stale after workspace edits and reviewed marks survive changed contents |
| [panel-tools-013](#panel-tools-013) | P2 | source-proven | missing-capability | Changes lacks full diff navigation and comparison controls present in T3 |
| [panel-tools-014](#panel-tools-014) | P2 | source-proven | defect | Stage and unstage failures are discarded |
| [panel-tools-016](#panel-tools-016) | P2 | source-proven | missing-capability | Terminal surfaces cannot split into simultaneous shells |
| [panel-tools-017](#panel-tools-017) | P2 | source-proven | missing-capability | Terminal output has no URL or file-path opening flow |
| [panel-tools-018](#panel-tools-018) | P2 | inferred | interaction | Panel sizing cannot adapt to narrow layouts or recover a smaller width after wide tools |
| [pr-navigation-02](#pr-navigation-02) | P2 | source-proven | interaction | The main PR affordances bypass the native PR view |
| [pr-navigation-03](#pr-navigation-03) | P2 | source-proven | missing-capability | There is no standalone PR inbox or project-scoped PR browsing destination |
| [pr-navigation-04](#pr-navigation-04) | P2 | source-proven | partial-capability | The native PR target is the current branch, so links and multiple PRs cannot become independent native tabs |
| [pr-navigation-05](#pr-navigation-05) | P2 | source-proven | defect | A known PR without a URL still presents clickable controls that silently do nothing |
| [pr-navigation-06](#pr-navigation-06) | P2 | unverified | verification-gap | The reported sidebar click failure lacks end-to-end verification at the actual entry points |
| [pr-operations-002](#pr-operations-002) | P2 | source-proven | defect | Reload discards the last good PR and replaces it with a transient error |
| [pr-operations-003](#pr-operations-003) | P2 | source-proven | defect | Authentication and retry guidance do not form a working recovery flow |
| [pr-operations-004](#pr-operations-004) | P2 | source-proven | defect | Rewrite guide displays the oldest matching guide |
| [pr-operations-006](#pr-operations-006) | P2 | source-proven | defect | Timeline drops review replies, resolution state and quoted code |
| [pr-operations-007](#pr-operations-007) | P2 | source-proven | defect | Capped PR data is presented as a complete conversation and check result |
| [pr-operations-008](#pr-operations-008) | P2 | source-proven | partial-capability | Checks stop at names and icons without a route to failure details |
| [pr-operations-009](#pr-operations-009) | P2 | source-proven | partial-capability | Descriptions, comments and guides render as raw text |
| [pr-operations-010](#pr-operations-010) | P2 | source-proven | missing-capability | No native comment, reply, review verdict or resolution workflow |
| [pr-operations-011](#pr-operations-011) | P2 | source-proven | missing-capability | PR metadata cannot be edited or managed in the native panel |
| [pr-operations-012](#pr-operations-012) | P2 | source-proven | partial-capability | Agent-backed PR operations have no panel-level pending, success or failure lifecycle |
| [pr-operations-013](#pr-operations-013) | P2 | source-proven | missing-capability | Lifecycle actions lack inverse operations and branch maintenance |
| [pr-operations-014](#pr-operations-014) | P2 | inferred | partial-capability | PR action prompts lose canonical host and repository identity |
| [pr-operations-015](#pr-operations-015) | P2 | inferred | defect | Concurrent PR reads can land out of order and retire the current tab |
| [pr-operations-016](#pr-operations-016) | P2 | source-proven | defect | PR read workers have no effective cancellation or deadline |
| [pr-operations-017](#pr-operations-017) | P2 | source-proven | partial-capability | Review verdicts and empty-body approvals are absent from the snapshot |
| [pr-operations-018](#pr-operations-018) | P2 | source-proven | defect | PR panel omits the checkout-versus-host sync warning |
| [pr-operations-019](#pr-operations-019) | P2 | source-proven | partial-capability | Guide is a comment convention without revision-aware review workflow |
| [pr-operations-021](#pr-operations-021) | P2 | source-proven | partial-capability | Merge controls ignore permissions, allowed methods and readiness |
| [projects-navigation-002](#projects-navigation-002) | P2 | source-proven | defect | Reopen failures produce a log entry without a visible error |
| [projects-navigation-003](#projects-navigation-003) | P2 | source-proven | defect | Undo Park does not restore group membership or a dissolved group |
| [projects-navigation-004](#projects-navigation-004) | P2 | source-proven | partial-capability | Bulk Park discards individual failures and clears the selection |
| [projects-navigation-005](#projects-navigation-005) | P2 | source-proven | partial-capability | Sidebar search matches titles only |
| [projects-navigation-006](#projects-navigation-006) | P2 | source-proven | defect | Keyboard search navigation does not reveal the highlighted result |
| [projects-navigation-007](#projects-navigation-007) | P2 | source-proven | defect | Live search results can change the Enter target or make Enter a no-op |
| [projects-navigation-008](#projects-navigation-008) | P2 | source-proven | partial-capability | Project scope has no search, multi-project selection, or persistence |
| [projects-navigation-009](#projects-navigation-009) | P2 | source-proven | interaction | Opening a thread outside the project filter leaves its sidebar selection hidden |
| [projects-navigation-010](#projects-navigation-010) | P2 | source-proven | missing-capability | Pinned rows cannot be reordered directly |
| [projects-navigation-011](#projects-navigation-011) | P2 | source-proven | partial-capability | Multi-selection applies only to Park and is absent from slim rows |
| [projects-navigation-012](#projects-navigation-012) | P2 | source-proven | interaction | Project editing and removal lack a contextual navigation entry point |
| [projects-navigation-013](#projects-navigation-013) | P2 | source-proven | partial-capability | New thread on the same branch or worktree requires manual reselection |
| [projects-navigation-014](#projects-navigation-014) | P2 | source-proven | intentional-difference | Park combines session closure and sidebar settlement, unlike T3 |
| [PS-03](#ps-03) | P2 | source-proven | defect | Provider version checks and updates target local binaries while a remote server is active |
| [PS-04](#ps-04) | P2 | source-proven | partial-capability | Provider setup stops at CLI presence and update status |
| [PS-05](#ps-05) | P2 | source-proven | partial-capability | An unavailable native conversation has no same-thread context recovery path |
| [PS-06](#ps-06) | P2 | source-proven | missing-capability | Provider choices do not include T3's additional drivers or named provider instances |
| [PS-07](#ps-07) | P2 | source-proven | partial-capability | Claude Auto and Don't ask are available per session but absent from new-thread defaults |
| [PS-08](#ps-08) | P2 | source-proven | missing-capability | Provider defaults have no project override or fallback-model settings |
| [PS-10](#ps-10) | P2 | source-proven | intentional-difference | Remote access supports SSH, not T3's pairing links and alternate routes |
| [quality-correction-003](#quality-correction-003) | P2 | runtime-reproduced | interaction | Empty browser address entry lacks a clear editable-region affordance |
| [quality-correction-004](#quality-correction-004) | P2 | runtime-reproduced | defect | Native PR read errors run beyond the panel and hide their cause and recovery text |
| [quality-correction-005](#quality-correction-005) | P2 | source-proven | partial-capability | Thread context lacks an independent compact details surface |
| [quote-attachments-002](#quote-attachments-002) | P2 | runtime-reproduced | interaction | Quote previews hide their full contents with no disclosure |
| [quote-attachments-003](#quote-attachments-003) | P2 | source-proven | interaction | The Cite toolbar is pointer-only and does not observe keyboard selection |
| [quote-attachments-004](#quote-attachments-004) | P2 | source-proven | defect | Attach message as context silently does nothing for long messages |
| [quote-attachments-005](#quote-attachments-005) | P2 | source-proven | defect | Ordinary blockquote text is reclassified as an uneditable citation on restore |
| [quote-attachments-006](#quote-attachments-006) | P2 | source-proven | defect | A quote-only prompt renders differently when a file is attached |
| [quote-attachments-008](#quote-attachments-008) | P2 | source-proven | defect | Valid image attachments can show a permanent false Failed tile |
| [quote-attachments-009](#quote-attachments-009) | P2 | source-proven | interaction | Draft file previews and sent image previews lack keyboard activation |
| [quote-attachments-010](#quote-attachments-010) | P2 | source-proven | intentional-difference | Attachment history stores mutable paths rather than retained content |
| [quote-attachments-011](#quote-attachments-011) | P2 | inferred | interaction | The attachment shelf has no bounded overflow policy |
| [scroll-quality-001](#scroll-quality-001) | P2 | source-proven | defect | Scrollbar navigation does not consistently take ownership from automatic scrolling |
| [scroll-quality-002](#scroll-quality-002) | P2 | source-proven | defect | Scrollbar track clicks use the wrong scroll distance |
| [scroll-quality-003](#scroll-quality-003) | P2 | source-proven | defect | The latest button undercounts hidden content when send runway remains |
| [scroll-quality-004](#scroll-quality-004) | P2 | source-proven | interaction | Expanding finished work at the tail does not preserve the clicked heading |
| [scroll-quality-005](#scroll-quality-005) | P2 | source-proven | defect | Keyboard End retains the send spacer that the latest button removes |
| [scroll-quality-006](#scroll-quality-006) | P2 | source-proven | defect | Find can compete with an existing jump or live follow inside a visible row |
| [scroll-quality-007](#scroll-quality-007) | P2 | inferred | partial-capability | Width changes lack a reading anchor inside a tall row |
| [scroll-quality-008](#scroll-quality-008) | P2 | inferred | defect | Saved jump, runway and resize indices are not rebased when rows change |
| [scroll-quality-009](#scroll-quality-009) | P2 | source-proven | defect | Page navigation does not subtract the pinned prompt's covered area |
| [subagents-background-01](#subagents-background-01) | P2 | source-proven | defect | Opening an evicted agent through its transcript row never requests saved history |
| [subagents-background-04](#subagents-background-04) | P2 | source-proven | partial-capability | Agent Panel views omit coverage, decision guidance and full status |
| [subagents-background-05](#subagents-background-05) | P2 | source-proven | interaction | Generic thread opening can replace Main with the first pending child |
| [subagents-background-06](#subagents-background-06) | P2 | source-proven | defect | Background Stop never clears its pending marker or offers retry after failure |
| [subagents-background-07](#subagents-background-07) | P2 | source-proven | partial-capability | Background-work Stop excludes every subagent it counts |
| [subagents-background-08](#subagents-background-08) | P2 | source-proven | partial-capability | Background task views disagree about which work exists |
| [subagents-background-09](#subagents-background-09) | P2 | source-proven | partial-capability | Agent inspection tabs disappear on restart despite the Panel persistence contract |
| [transcript-001](#transcript-001) | P2 | source-proven | partial-capability | Find cannot discover text hidden by turn or tool disclosures |
| [transcript-002](#transcript-002) | P2 | inferred | missing-capability | Long transcript retention has no equivalent of earlier-turn paging |
| [transcript-003](#transcript-003) | P2 | source-proven | partial-capability | Successful turns erase the visible plan history |
| [transcript-004](#transcript-004) | P2 | source-proven | interaction | Opening one tool removes the entire work-group height cap |
| [transcript-006](#transcript-006) | P2 | source-proven | partial-capability | Citations and copied text do not retain a navigable source message |
| [transcript-007](#transcript-007) | P2 | source-proven | missing-capability | Markdown math is displayed as code instead of typeset formulas |
| [transcript-008](#transcript-008) | P2 | source-proven | missing-capability | Transcript Markdown has no inline audio or video playback |
| [BROWSER-15](#browser-15) | P3 | source-proven | interaction | Errors occupy an inline text row with repetitive recovery copy |
| [composer-12](#composer-12) | P3 | source-proven | missing-capability | There is no prompt stash independent of the active draft |
| [composer-13](#composer-13) | P3 | source-proven | intentional-difference | The native composer lacks rich Markdown and code-block editing |
| [composer-14](#composer-14) | P3 | source-proven | missing-capability | There is no integrated voice-input workflow |
| [composer-15](#composer-15) | P3 | source-proven | missing-capability | A first prompt cannot fan out to multiple chosen models |
| [coverage-quality-06](#coverage-quality-06) | P3 | source-proven | partial-capability | Factory schedules do not offer one-time or sub-hour scheduled prompts |
| [coverage-quality-07](#coverage-quality-07) | P3 | inferred | missing-capability | No first-class guest thread participation was found |
| [coverage-quality-08](#coverage-quality-08) | P3 | source-proven | missing-capability | T3 device panels have no Ferrite panel counterpart |
| [motion-004](#motion-004) | P3 | source-proven | motion | Popover and menu entrances are globally disabled |
| [motion-009](#motion-009) | P3 | source-proven | partial-capability | Long programmatic scrolls can snap before their final ease |
| [motion-011](#motion-011) | P3 | source-proven | motion | Several transient elements animate in but disappear immediately |
| [pr-operations-005](#pr-operations-005) | P3 | source-proven | defect | Timeline reverses the backend order and has no ordering control |
| [PS-09](#ps-09) | P3 | source-proven | intentional-difference | Appearance is deliberately dark-only with fixed font families |
| [transcript-005](#transcript-005) | P3 | source-proven | partial-capability | Pin as chapter is a transient label without chapter navigation |

## BROWSER-01

New-window links and scripted popups are silently discarded

P1. defect. source-proven. Disposition `repair-or-quality`.

Ordinary website links that request another tab do nothing. Popup-based authentication cannot complete through this handler.

Recommendation. Separate external website navigation policy from sealed visualization policy. Dispatch ordinary new-tab requests through the service and explicitly support or explain authentication popups.

Acceptance check. In one local fixture, same-tab navigation, target=_blank, middle-click, and a scripted popup each produce the documented destination or a visible actionable refusal. The opener-based fixture can report a successful callback.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-pane/src/visual/web/cef/handlers.rs:151-165](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L151-L165)
- [crates/ferrite-pane/src/visual/web/cef/handlers.rs:384-401](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L384-L401)
- [crates/ferrite-pane/src/visual/web/cef/handlers.rs:442-450](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L442-L450)
- [apps/desktop/src/preview/Manager.ts:488-504](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L488-L504)
- [apps/desktop/src/preview/Manager.ts:1637-1658](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L1637-L1658)

## BROWSER-02

Website JavaScript dialogs are always dismissed

P1. defect. source-proven. Disposition `repair-or-quality`.

An action gated by confirm cannot receive affirmative consent; prompt cannot collect input, and alert messages never reach the operator.

Recommendation. Keep sealed visualization restrictions, but supply a native or GPUI dialog bridge for external pages, with a deliberate suppression state when appropriate.

Acceptance check. A local confirm fixture executes its positive branch after the user chooses OK; Cancel returns false. Prompt returns entered text, and alert is visible. No website can replace the app window.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-pane/src/visual/web/cef/handlers.rs:151-165](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L151-L165)
- [crates/ferrite-pane/src/visual/web/cef/handlers.rs:643-661](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L643-L661)

## composer-01

An idle send can clear the only draft before admission fails

P1. defect. source-proven. Disposition `repair-or-quality`.

An unsent prompt and its staged context can disappear when the user sends to an unavailable workspace or a session that cannot start. This is a conditional loss after accepting Submit, not evidence of general transcript loss.

Recommendation. Make send admission return an explicit outcome or recover the full snapshot on every definite refusal. Preserve uncertain-send status separately to avoid accidental duplicate sends.

Acceptance check. Send a unique multiline prompt with a file and quote after forcing each early refusal. The prompt and context remain editable, the refusal is visible, no user message is falsely shown as delivered, and a later retry sends exactly once. Type a second draft during failure and verify both remain recoverable.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:5552-5554](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L5552-L5554)
- [crates/ferrite-pane/src/composer.rs:516-527](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L516-L527)
- [crates/ferrite-core/src/engine.rs:2537-2578](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L2537-L2578)
- [crates/ferrite-core/src/engine.rs:989-993](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L989-L993)
- [crates/ferrite-core/src/cockpit.rs:1747-1758](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L1747-L1758)
- [apps/web/src/components/chat/useComposerSend.ts:1404-1453](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1404-L1453)

## composer-02

A new unsent pane has no durable draft record

P1. defect. runtime-reproduced. Disposition `repair-or-quality`.

A first prompt composed before a thread exists is lost when the app closes. Existing-thread drafts persist, making the difference hard to anticipate. This is loss of unsent editor state before the user accepts Submit, not loss of an accepted prompt or transcript.

Recommendation. Persist DraftId-owned records separately from thread drafts, including the launch binding, and migrate ownership atomically on first send.

Acceptance check. Create two unsent new drafts with different models, branches and attachments. Quit normally and reopen. Both return unchanged; sending one promotes only that draft and does not duplicate it on a second restart.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:2204-2228](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L2204-L2228)
- [crates/ferrite-core/src/roster.rs:19-48](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/roster.rs#L19-L48)
- [crates/ferrite-core/src/drafts.rs:12-17](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/drafts.rs#L12-L17)
- [crates/ferrite-core/src/draft.rs:27-35](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/draft.rs#L27-L35)
- [crates/ferrite-cockpit/src/cockpit/drafts.rs:28-64](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/drafts.rs#L28-L64)
- [apps/web/src/composerDraftStore.ts:2263-2371](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2263-L2371)
- [apps/web/src/composerDraftStore.ts:2580-2641](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2580-L2641)

Review note. A never-sent Cmd+N draft vanished after owned window SIGTERM/restart after autosave; an existing-thread draft persisted and restored as the control. Normal Quit, launch context and attachments were not tested.

Runtime evidence:

- [draft-lifecycle-runtime-correction.md](reports/draft-lifecycle-runtime-correction.md)
- [03-unsent-draft-before.png](evidence/draft-lifecycle-correction/03-unsent-draft-before.png)
- [06-new-draft-after-restart.png](evidence/draft-lifecycle-correction/06-new-draft-after-restart.png)
- [drafts-before-restart.json](evidence/draft-lifecycle-correction/drafts-before-restart.json)

## composer-03

Ordinary local attachments are not transferred to a remote machine

P1. defect. source-proven. Disposition `repair-or-quality`.

A local file or pasted screenshot can look attached in a remote thread while the agent receives only a client-local path that does not exist on its machine.

Recommendation. Represent ordinary attachment ownership and transfer state explicitly. Stage bytes on the destination before admitting a send, retain progress/errors, and never reinterpret client absolute paths as remote files.

Acceptance check. Attach a client-only PNG and text file to a remote thread using all three intake methods. The agent can read their exact contents; upload failure retains the draft and offers retry/removal. A coincidentally identical remote path must not substitute different bytes.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:8525-8541](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L8525-L8541)
- [crates/ferrite-pane/src/composer.rs:572-618](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L572-L618)
- [crates/ferrite-pane/src/prompt_drop.rs:36-45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/prompt_drop.rs#L36-L45)
- [crates/ferrite-core/src/prompt_files.rs:15-24](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/prompt_files.rs#L15-L24)
- [crates/ferrite-core/src/cockpit.rs:1747-1758](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L1747-L1758)
- [crates/ferrite-core/src/providers/codex/wire.rs:1276-1293](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex/wire.rs#L1276-L1293)
- [crates/ferrite-core/src/providers/claude/wire.rs:28-55](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/claude/wire.rs#L28-L55)
- [apps/web/src/lib/attachmentUploadQueue.ts:135-161](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135-L161)
- [apps/web/src/components/chat/useComposerSend.ts:1110-1114](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1110-L1114)

## coverage-quality-01

UI interaction tests do not gate ordinary PR CI

P1. verification-gap. source-proven. Disposition `verification-gap`.

Core tests and an app build can be green while cockpit and pane interaction tests never execute.

Recommendation. Run the affected UI crate test targets in required PR CI; retain narrow local verification.

Acceptance check. A deliberate failing cockpit interaction regression fails the required PR check, and restoring it makes that same check green.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [.github/workflows/ci.yml:73-93](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/.github/workflows/ci.yml#L73-L93)
- [crates/ferrite-cockpit/Cargo.toml:56-65](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/Cargo.toml#L56-L65)

## input-focus-001

Host action bindings take browser and empty-panel input before their local handlers

P1. defect. source-proven. Disposition `repair-or-quality`.

Browser forms and keyboard navigation cannot rely on normal page input. Enter can send or queue the main conversation draft while the user is editing a web page. Paste targets that draft and takes focus away from the page.

Recommendation. Give browser content an explicit input context and route its editing actions to the browser before the cockpit acts. Keep page keys and host navigation shortcuts separate. Guard Submit with the actual sending control or explicit submit intent, so unrelated focused content cannot send its draft.

Acceptance check. With a fake provider and a local form, leave a sentinel unsent in the main composer. Click the page input and exercise Enter, Tab, Shift-Tab, Escape, copy and paste. The form receives intended keys, the draft remains unchanged, no provider send occurs, and only explicit host commands navigate away.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [vendor/gpui-pre/src/window.rs:6152-6168](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/window.rs#L6152-L6168)
- [vendor/gpui-pre/src/window.rs:6539-6557](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/window.rs#L6539-L6557)
- [crates/ferrite-pane/src/visual/element.rs:265-287](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L265-L287)
- [crates/ferrite-cockpit/src/cockpit/panel/preview.rs:383](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L383)
- [crates/ferrite-cockpit/src/cockpit/panel/preview.rs:498-520](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L498-L520)
- [crates/ferrite-ui/src/keymap.rs:47](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L47)
- [crates/ferrite-ui/src/keymap.rs:90-93](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L90-L93)
- [crates/ferrite-ui/src/keymap.rs:179-192](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L179-L192)
- [crates/ferrite-ui/src/keymap.rs:304-315](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L304-L315)
- [crates/ferrite-cockpit/src/cockpit/palette.rs:1378-1402](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/palette.rs#L1378-L1402)
- [crates/ferrite-ui/src/keymap.rs:537](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L537)
- [crates/ferrite-cockpit/src/cockpit.rs:5486-5596](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L5486-L5596)
- [crates/ferrite-cockpit/src/cockpit.rs:9292-9335](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L9292-L9335)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:670-691](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L670-L691)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:716-742](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L716-L742)
- [crates/ferrite-cockpit/src/cockpit.rs:10239-10248](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L10239-L10248)
- [crates/ferrite-ui/src/keymap.rs:185-189](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L185-L189)
- [crates/ferrite-cockpit/src/cockpit/tests/tools.rs:190-220](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/tools.rs#L190-L220)
- [apps/desktop/src/preview/Manager.ts:514-534](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L514-L534)
- [apps/desktop/src/preview/Manager.ts:1509-1583](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L1509-L1583)
- [apps/web/src/components/RightPanelTabs.tsx:438-460](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L438-L460)
- [apps/web/src/components/RightPanelTabs.tsx:475-481](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L475-L481)

Related record [input-focus-002](reports/input-focus.md). The empty panel advertises a keyboard launcher that cannot receive its Enter key under the production binding table. An unsent main draft is exposed to the same unintended-submit path as browser content.

Additional acceptance check. An integration test uses bind_production_keys, clicks the blank launcher, sends ArrowDown and Enter as real key events, and asserts exactly the chosen surface opens with zero main-composer sends.

Review note. The same action-before-key-listener boundary also causes input-focus-002. Browser Enter can send the conversation draft and browser Escape can close the surface. The empty-panel branch was reproduced through real GPUI dispatch: Enter sent PANEL_ENTER_SENTINEL and left the launcher open. The browser branch was not exercised in Chromium. This combined record retains source-proven confidence to avoid presenting both branches as runtime-reproduced.

Runtime evidence:

- [runtime-interactions.md](reports/runtime-interactions.md)
- [09-panel-before-enter.png](evidence/headless/09-panel-before-enter.png)
- [10-panel-after-enter.png](evidence/headless/10-panel-after-enter.png)
- [provider-host.jsonl](evidence/headless/provider-host.jsonl)

## input-focus-003

Right-panel tabs, close buttons and PR controls have no keyboard focus or accessible control roles

P1. interaction. source-proven. Disposition `repair-or-quality`.

Keyboard-only users cannot operate much of the right panel. Assistive technology cannot discover the raw controls as named buttons or tabs. A visible hover and an on_click handler do not make a GPUI Div focusable.

Recommendation. Use named, focusable native controls for panel actions, with a single tab-order policy. Expose tab state and PR segment selection to accessibility. Preserve focus through redraws rather than just adding handles to controls the focus pump will reclaim. Increase close hit area while retaining the visual icon size.

Acceptance check. Operate tab switching, close, add, maximize, PR views and PR link without a pointer. Inspect the accessibility tree for names, roles and selected states. Repeated background updates preserve the focused control and Enter/Space executes only that control.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:487-575](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L487-L575)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:578-619](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L578-L619)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:314-324](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L314-L324)
- [crates/ferrite-pane/src/preview.rs:689-707](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L689-L707)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:517-533](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L517-L533)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:578-603](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L578-L603)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:609-633](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L609-L633)
- [crates/ferrite-ui/src/components.rs:29-39](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/components.rs#L29-L39)
- [crates/ferrite-ui/src/components.rs:144-158](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/components.rs#L144-L158)
- [crates/ferrite-ui/src/theme.rs:1019](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/theme.rs#L1019)
- [vendor/gpui-pre/src/elements/div.rs:1876-1882](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/div.rs#L1876-L1882)
- [vendor/gpui-pre/src/elements/div.rs:2222-2241](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/div.rs#L2222-L2241)
- [vendor/gpui-pre/src/elements/div.rs:2992-3057](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/div.rs#L2992-L3057)
- [apps/web/src/components/RightPanelTabs.tsx:912-915](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L912-L915)
- [apps/web/src/components/RightPanelTabs.tsx:977-990](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L977-L990)
- [apps/web/src/components/RightPanelTabs.tsx:1408-1418](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L1408-L1418)
- [apps/web/src/components/preview/RightPanelResizeHandle.tsx:14-25](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/RightPanelResizeHandle.tsx#L14-L25)

## input-focus-004

Cmd+W on a PR or terminal can close the parent conversation instead of the active panel surface

P1. interaction. source-proven. Disposition `repair-or-quality`.

A familiar close command can remove the working conversation from view when the user intends to close a panel tab. The thread is parked, not deleted, but the navigation change is unrelated to the clicked surface.

Recommendation. Route close through a focused-area owner and active surface. Preserve the explicit command for closing a conversation, with a clearly named separate action or context.

Acceptance check. For each panel surface, focus its content, press close once and assert only its tab closes. The parent thread stays selected and unparked. Test the browser/file/review exceptions to preserve their existing behavior.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [crates/ferrite-ui/src/keymap.rs:364-367](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L364-L367)
- [crates/ferrite-cockpit/src/cockpit.rs:9148-9175](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L9148-L9175)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:282-293](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L282-L293)
- [crates/ferrite-terminal/src/view.rs:324-344](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-terminal/src/view.rs#L324-L344)
- [crates/ferrite-ui/src/keymap.rs:604-617](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L604-L617)
- [packages/shared/src/keybindings.ts:69-75](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L69-L75)
- [apps/web/src/components/chat/useRightPanelActions.ts:315-324](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useRightPanelActions.ts#L315-L324)

## input-focus-007

Typing y, n or a as the first character of an empty approval composer performs a permission action

P1. interaction. source-proven. Disposition `repair-or-quality`.

A user who starts a normal follow-up with "also", "no" or "yes" can answer a pending approval before sending a message. The a shortcut means Always. This is an intentional current interaction contract, but it conflicts with the live text-field affordance.

Recommendation. Keep bare approval letters in an explicitly focused approval region or non-editing mode. Use modified chords or ordinary focused buttons while the composer is editable. Preserve question and nonempty-draft behavior.

Acceptance check. With an approval pending and the composer empty, type "also check this", "no, change direction" and "yes, after tests" as ordinary key events. All text remains unsent and no approval response occurs. Explicit approval shortcuts and buttons still work.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [crates/ferrite-pane/src/pane/composer.rs:504-517](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L504-L517)
- [crates/ferrite-ui/src/keymap.rs:193-207](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L193-L207)
- [crates/ferrite-cockpit/src/cockpit.rs:5984-5993](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L5984-L5993)
- [crates/ferrite-cockpit/src/cockpit.rs:6012-6040](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L6012-L6040)
- [crates/ferrite-cockpit/src/cockpit/tests/composer_menus.rs:957-1027](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/composer_menus.rs#L957-L1027)
- [apps/web/src/components/chat/ComposerPendingApprovalActions.tsx:26-30](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L26-L30)
- [apps/web/src/components/chat/ComposerPendingApprovalActions.tsx:51-86](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L51-L86)

## input-focus-010

Files editor and filter lose keyboard focus when their edits redraw the cockpit

P1. defect. runtime-reproduced. Disposition `repair-or-quality`.

Code editing and file filtering can redirect subsequent typing into the main conversation. This is a concrete missing exemption in the current focus reconciliation, beyond the broader area-navigation gap in input-focus-005.

Recommendation. Preserve the active Files editor and filter as real focus owners. Prefer a general active-surface ownership check over another loosely related global exception. Replace the editing proof with a rendered click-and-type integration check in addition to the autosave test.

Acceptance check. In a scratch checkout, click a source line and type several separate key events across cockpit updates. Assert every character lands in the file, none enters the composer, and focus remains in the editor. Repeat in the file filter and while a transcript streams. No live user file is needed for this test.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/editor.rs:301-312](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/editor.rs#L301-L312)
- [crates/ferrite-cockpit/src/cockpit/tools/editor.rs:201-216](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/editor.rs#L201-L216)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:125-135](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L125-L135)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:228-239](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L228-L239)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:446-464](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L446-L464)
- [crates/ferrite-cockpit/src/cockpit.rs:10159-10162](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L10159-L10162)
- [crates/ferrite-cockpit/src/cockpit.rs:10220-10274](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L10220-L10274)
- [crates/ferrite-cockpit/src/cockpit/tests/files.rs:74-89](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/files.rs#L74-L89)
- [apps/web/src/components/files/FilePreviewPanel.browser.test.tsx:189-225](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.browser.test.tsx#L189-L225)
- [apps/web/src/lib/workspaceFocus.ts:120-138](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/workspaceFocus.ts#L120-L138)

Review note. Real clicks followed by separate printable keys sent xyz and sam to the main composer; the editor and filter remained unchanged. Failure was visible on the first post-click key.

Runtime evidence:

- [runtime-interactions.md](reports/runtime-interactions.md)
- [02-editor-before.png](evidence/headless/02-editor-before.png)
- [04-editor-next-keys.png](evidence/headless/04-editor-next-keys.png)
- [06-filter-next-keys.png](evidence/headless/06-filter-next-keys.png)

## panel-tools-001

A failed file save is silent and closing the file discards the edited buffer

P1. defect. source-proven. Disposition `repair-or-quality`.

An operator can lose unsaved work when the file becomes unwritable or its parent directory disappears. P0 follows this audit's data-loss rule; this is conditional, not a claim that ordinary saves fail.

Recommendation. Return structured save results, report failures, keep the tab and draft on failure, and make close/save decisions explicit. Use atomic replacement to avoid partial-write corruption.

Acceptance check. In a disposable checkout, deny writes after editing. Save displays the filesystem error. Closing preserves the exact draft and offers retry or explicit discard. Restore permission and verify that retry writes the draft.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/editor.rs:135-140](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/editor.rs#L135-L140)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:256-268](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L256-L268)
- [apps/web/src/components/files/fileSaveCoordinator.ts:67-88](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/fileSaveCoordinator.ts#L67-L88)
- [apps/web/src/components/files/fileSaveSessions.ts:46-59](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/fileSaveSessions.ts#L46-L59)

Review note. Conditional loss of an edited buffer remains urgent. P0 was too strong without a reproduced widespread trigger.

## panel-tools-002

Cached editor buffers can overwrite newer agent or other-thread changes

P1. defect. source-proven. Disposition `repair-or-quality`.

An agent and operator often edit the same checkout. A later save from an old file buffer can erase the agent's changes. Two Ferrite threads pointing at one checkout also create independent buffers.

Recommendation. Track disk revisions, refresh clean buffers on workspace changes, and block or reconcile conflicting saves. Coordinate writes by canonical workspace and file, not by thread alone.

Acceptance check. Open the same file in two threads sharing a checkout. Change a different line in each, and repeat with an external agent edit. No successful save silently removes the other edit; a conflict is visible and recoverable.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools.rs:20-26](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools.rs#L20-L26)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:159-191](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L159-L191)
- [crates/ferrite-cockpit/src/cockpit/tools/editor.rs:66-83](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/editor.rs#L66-L83)
- [apps/web/src/components/files/FilePreviewPanel.tsx:1322-1335](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1322-L1335)
- [apps/web/src/components/files/fileSaveSessions.ts:46-59](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/fileSaveSessions.ts#L46-L59)

Review note. A stale buffer can overwrite an agent's newer disk contents. This is a Ferrite integrity defect. The independent reviewer did not find T3 stale-write protection either, so it is not presented as a verified T3 parity advantage. A clean Markdown preview stayed stale on same-tab revisit after external replacement; close/reopen loaded new disk text. Dirty-buffer overwrite and save-conflict loss were not exercised.

Runtime evidence:

- [files-runtime-correction.md](reports/files-runtime-correction.md)
- [08-readme-revisited-after-external-change.png](evidence/files-correction/08-readme-revisited-after-external-change.png)
- [09-readme-reopened-after-close.png](evidence/files-correction/09-readme-reopened-after-close.png)

## panel-tools-003

The file tree never refreshes cached directory entries or Git status

P1. defect. source-proven. Disposition `repair-or-quality`.

Files created, deleted, or renamed by an agent are missing or stale in an already opened tree. Git colors remain based on its initial snapshot.

Recommendation. Invalidate directories and Git status on workspace mutation and expose Refresh. Keep expanded paths and selection while applying updates.

Acceptance check. With Files already open, create, rename, delete, and modify files through a terminal or agent. Tree rows and status update without restarting or changing threads; manual refresh also works.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/tree.rs:43-76](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/tree.rs#L43-L76)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:118-154](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L118-L154)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:314-325](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L314-L325)
- [apps/web/src/components/files/FileBrowserPanel.tsx:64-80](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FileBrowserPanel.tsx#L64-L80)

Review note. External add/delete left cached children unchanged even after collapse/re-expand. Git status coloring is not runtime-tested.

Runtime evidence:

- [files-runtime-correction.md](reports/files-runtime-correction.md)
- [10-tree-expanded-before-mutation.png](evidence/files-correction/10-tree-expanded-before-mutation.png)
- [12-tree-after-collapse-expand.png](evidence/files-correction/12-tree-after-collapse-expand.png)

## panel-tools-005

Nested file tabs overflow into an inaccessible clipped region

P1. interaction. runtime-reproduced. Disposition `repair-or-quality`.

Opening many files can put the active file's tab and close control offscreen. The unified panel strip's overflow controls do not control this separate strip.

Recommendation. Move editable files into the shared panel surface list. If retaining an inner strip, add overflow navigation and automatic active-tab reveal with the same keyboard and close semantics.

Acceptance check. Open 20 kept files at a narrow panel width. Reach, activate, and close every tab by mouse and keyboard. A newly opened or reactivated file always has a visible selected tab.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:467-543](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L467-L543)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:244-253](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L244-L253)
- [docs/adr/0039-the-panel-is-one-tabbed-surface-host.md:15-29](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/adr/0039-the-panel-is-one-tabbed-surface-host.md#L15-L29)
- [apps/web/src/rightPanelStore.ts:812-844](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L812-L844)

Review note. Eighteen kept inner tabs placed files-tab-17 at x=3250 in a 1440px window without strip navigation. The file tree still activates paths; the failure concerns strip access and hidden close controls.

Runtime evidence:

- [files-runtime-correction.md](reports/files-runtime-correction.md)
- [26-kept-tabs-overflow.png](evidence/files-correction/26-kept-tabs-overflow.png)

## panel-tools-009

Changes cuts each file at 120 displayed lines with no expansion action

P1. defect. source-proven. Disposition `repair-or-quality`.

A primary review view hides later changes in a large file. A user can mark the file reviewed without having a way to inspect its omitted hunks in that view.

Recommendation. Give Changes a full diff renderer with lazy hunk loading or virtualization. Keep summary caps only in explicitly abbreviated Overview cards.

Acceptance check. Create a diff with more than 200 changed lines and a distinctive deletion near the end. The user can inspect that deletion in Changes and reach every omitted hunk before marking the file reviewed.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:21-26](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/hunks.rs#L21-L26)
- [crates/ferrite-cockpit/src/cockpit/tools/changes.rs:819-845](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/changes.rs#L819-L845)
- [apps/web/src/components/DiffPanel.tsx:1505-1525](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L1505-L1525)

## panel-tools-015

Terminal startup failures leave no visible result

P1. defect. source-proven. Disposition `repair-or-quality`.

Terminal is a primary debugging fallback. A failed shell launch makes its menu item and shortcut look broken.

Recommendation. Represent terminal startup as loading, running, or failed. Retain the failed surface so the operator can read the error and retry.

Acceptance check. Force a shell or cwd startup failure in a disposable thread. Both the menu and shortcut display the same actionable failure and recover after fixing the cause.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:93-116](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/terminal.rs#L93-L116)
- [apps/web/src/components/ThreadTerminalDrawer.tsx:194-207](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L194-L207)

## pr-navigation-01

Clicking another thread's PR routes to a panel that is not shown

P1. defect. runtime-reproduced. Disposition `repair-or-quality`.

A plain click on the underlined PR badge of a nonfocused thread does not reveal its destination. The browser request targets that thread, while the visible panel remains attached to the focused thread. If browser ownership is unavailable, attachment fails and its error is also stored under the hidden target. This directly explains a class of apparent dead sidebar links.

Recommendation. Route an explicit PR click through a destination-aware navigation operation that makes the clicked thread or selected PR visible. Keep agent-initiated browser reveals separate from explicit user navigation.

Acceptance check. With A focused and B visible in the sidebar, click B's PR number. B's exact PR becomes visible on that click. Repeat with B without a board pane, with the panel hidden, and from the empty board. A modifier click opens the external URL without switching the thread.

[Full investigation and reproduction conditions](reports/pr-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:12806-12819](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12806-L12819)
- [crates/ferrite-cockpit/src/cockpit.rs:1826-1831](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L1826-L1831)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:218-240](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L218-L240)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:637-647](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L637-L647)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:204-211](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L204-L211)
- [crates/ferrite-cockpit/src/cockpit/nav_list.rs:327-337](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_list.rs#L327-L337)
- [crates/ferrite-core/src/server/browser.rs:116-130](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/server/browser.rs#L116-L130)
- [crates/ferrite-core/src/server/browser.rs:146-165](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/server/browser.rs#L146-L165)
- [apps/web/src/components/sidebar/SidebarThreadRow.tsx:962-969](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969)

Review note. Actual production gh discovery in two scratch repositories plus rendered badge clicks reproduced A selected / B clicked / no visible destination. The focused-thread badge was a positive control. CEF page load was not tested.

Runtime evidence:

- [pr-entry-runtime-correction.md](reports/pr-entry-runtime-correction.md)
- [cross-thread-verdict.json](evidence/pr-entry-correction/cross-thread-verdict.json)
- [18-cross-before.png](evidence/pr-entry-correction/18-cross-before.png)
- [19-cross-after.png](evidence/pr-entry-correction/19-cross-after.png)

## pr-operations-001

PR detail stays stale after status refresh and agent actions

P1. defect. source-proven. Disposition `repair-or-quality`.

Users cannot trust the displayed checks, comments, description, or generated guide to reflect the current PR. A running check can remain running after completion.

Recommendation. Give detail a refresh command and invalidation keyed by repository, PR and head/revision. Refresh activity after writes and checks while pending. Preserve a timestamp and stale state.

Acceptance check. Open a PR with a pending check, finish the check remotely, add a comment, and invoke Refresh PR status. The same panel shows the new result and comment. A new guide appears after its agent completes without reopening the PR.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:283-305](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L283-L305)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:319-335](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L319-L335)
- [crates/ferrite-cockpit/src/cockpit/palette.rs:1302-1305](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/palette.rs#L1302-L1305)
- [crates/ferrite-cockpit/src/cockpit.rs:1813-1823](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L1813-L1823)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:254-266](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L254-L266)
- [apps/web/src/components/pullRequest/usePullRequestDetail.ts:65-84](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L65-L84)
- [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:624-676](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L624-L676)

Review note. Both menu and palette Refresh PR status left same-number/same-lifecycle detail checks/body/comments stale while lightweight state updated. Re-entry and OPEN→MERGED refresh worked. Agent-action invalidation is not runtime-tested.

Runtime evidence:

- [pr-entry-runtime-correction.md](reports/pr-entry-runtime-correction.md)
- [refresh-verdict.json](evidence/pr-entry-correction/refresh-verdict.json)
- [14-refreshed-stale-summary.png](evidence/pr-entry-correction/14-refreshed-stale-summary.png)
- [15-overview-fresh-checks.png](evidence/pr-entry-correction/15-overview-fresh-checks.png)

## pr-operations-020

Create PR is unavailable for branch work absent from the current thread transcript

P1. defect. source-proven. Disposition `repair-or-quality`.

A user who opens a prepared branch, makes manual edits, or imports work from another session cannot use the native Create PR button unless this thread has recorded file edits.

Recommendation. Offer Create PR from repository state or a stable command, independent of transcript-derived change cards. Preserve the current agent handoff if that is the chosen creation design.

Acceptance check. Open a fresh thread on a branch with an ahead-of-base commit created outside Ferrite. Create PR is discoverable and targets that branch. No synthetic agent edit is required.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/changes.rs:19-45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/changes.rs#L19-L45)
- [crates/ferrite-cockpit/src/cockpit/panel/changes.rs:114-133](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/changes.rs#L114-L133)
- [crates/ferrite-cockpit/src/facts.rs:85-87](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/facts.rs#L85-L87)
- [crates/ferrite-core/src/digest.rs:355-376](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/digest.rs#L355-L376)
- [apps/web/src/components/GitActionsControl.logic.ts:117-136](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/GitActionsControl.logic.ts#L117-L136)
- [apps/web/src/components/GitActionsControl.logic.ts:164-174](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/GitActionsControl.logic.ts#L164-L174)

## projects-navigation-001

Rename on Parked and Snoozed rows creates an editor that is never drawn

P1. defect. source-proven. Disposition `repair-or-quality`.

An advertised rename action cannot be completed visibly on either slim shelf. The root also attempts to focus the absent editor, which creates a keyboard reliability risk.

Recommendation. Pass the same editable title into full and slim rows. Focus it only after the row has rendered. Keep Enter and Escape on the visible editor.

Acceptance check. Right-click a Parked row and a Snoozed row, choose Rename, and see the existing title in a focused field. Type, save, reopen the app, and confirm the saved title. Escape must cancel without swallowing subsequent navigation keys.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:431-434](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L431-L434)
- [crates/ferrite-cockpit/src/cockpit/nav_list.rs:285-295](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_list.rs#L285-L295)
- [crates/ferrite-cockpit/src/nav/list.rs:188-197](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/nav/list.rs#L188-L197)
- [crates/ferrite-cockpit/src/cockpit.rs:3392-3409](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L3392-L3409)
- [apps/web/src/components/sidebar/SidebarThreadRow.tsx:1033-1039](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1033-L1039)

Review note. Rendered Snoozed-row Rename closed the menu but displayed no editor or rename hint; the stored title remained unchanged. Parked remains source-only. Working-row commit was inconclusive and is not counted as a save failure.

Runtime evidence:

- [navigation-runtime-correction.md](reports/navigation-runtime-correction.md)
- [26-snoozed-row-menu.png](evidence/navigation-correction/26-snoozed-row-menu.png)
- [27-snoozed-rename-attempt.png](evidence/navigation-correction/27-snoozed-rename-attempt.png)

## PS-01

Approval cards can state the wrong Codex sandbox

P1. defect. source-proven. Disposition `repair-or-quality`.

The operator can approve a command while the card claims a more restrictive sandbox than the process actually has. This is an incorrect permission description, not evidence that Ferrite escapes the provider sandbox.

Recommendation. Project the negotiated sandbox through session events, thread state and the wire snapshot, and render that value. Keep requested defaults distinct from effective values.

Acceptance check. With a stub Codex handshake returning workspaceWrite while Settings is CLI default, an actual clicked approval card says workspace-write. Change the default to read-only and verify the existing card remains workspace-write. Repeat for a remote thread and dangerFullAccess. No real destructive command is needed.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/decisions.rs:65-79](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/decisions.rs#L65-L79)
- [crates/ferrite-core/src/settings.rs:365-372](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/settings.rs#L365-L372)
- [crates/ferrite-pane/src/decision.rs:113-123](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/decision.rs#L113-L123)
- [crates/ferrite-core/src/spawner.rs:453-464](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/spawner.rs#L453-L464)
- [crates/ferrite-core/src/providers/codex.rs:504-522](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex.rs#L504-L522)
- [crates/ferrite-core/src/providers/codex.rs:239-251](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex.rs#L239-L251)
- [crates/ferrite-core/src/providers/codex.rs:1048-1055](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex.rs#L1048-L1055)

## PS-02

Remote permission and server defaults save to the local machine

P1. defect. source-proven. Disposition `repair-or-quality`.

On an SSH machine, settings can appear saved while new remote sessions continue using the remote server's old permission and sandbox defaults. Factory concurrency and other server-side preferences have the same ownership problem.

Recommendation. Separate window preferences from server settings. Add a targeted server-settings read/write operation with an acknowledgement and display the machine whose defaults are edited. Do not silently synchronize unrelated local values.

Acceptance check. Use two isolated settings directories with different permission/sandbox defaults. Through a remote window change one default, create a thread and inspect its provider launch arguments/handshake. Only the selected machine changes, relaunch retains it, and denied/offline writes display an unsaved state.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite/src/lib.rs:274-301](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite/src/lib.rs#L274-L301)
- [crates/ferrite/src/lib.rs:394-406](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite/src/lib.rs#L394-L406)
- [crates/ferrite-cockpit/src/cockpit.rs:4319-4334](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4319-L4334)
- [crates/ferrite-core/src/cockpit.rs:2477-2488](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L2477-L2488)
- [crates/ferrite-core/src/server.rs:784-789](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/server.rs#L784-L789)
- [crates/ferrite-core/src/server.rs:1042-1053](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/server.rs#L1042-L1053)
- [crates/ferrite-core/src/cockpit.rs:3216-3224](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L3216-L3224)
- [apps/web/src/hooks/useSettings.ts:602](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useSettings.ts#L602)
- [apps/web/src/components/settings/SettingsScopeContext.tsx:58](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsScopeContext.tsx#L58)

## quality-correction-001

Agent transcript prose clips at ordinary panel width

P1. defect. runtime-reproduced. Disposition `repair-or-quality`.

At a 420px panel, child transcript sentences lose their endings beyond the right edge. Maximizing to 960px reveals the complete text. Output arrival alone does not make the agent screen readable.

Recommendation. Measure and wrap child prose against the actual panel content width; retain reading position across maximize, restore and resize. The precise layout cause remains to be isolated.

Acceptance check. Open a child with ordinary multi-sentence output at minimum and ordinary widths. Every character remains readable without maximizing; resizing and restoring reflow existing output without waiting for another event.

[Full investigation and reproduction conditions](reports/quality-correction.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-225](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L203-L225)
- [crates/ferrite-pane/src/transcript.rs:1527](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1527)
- [crates/ferrite-pane/src/transcript.rs:2955](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2955)

Review note. Parent inspected both screenshots. Source references locate the rendering path; they are not a proven root cause or proposed patch.

Runtime evidence:

- [agent-live-correction.md](reports/agent-live-correction.md)
- [13-completed-agent.png](evidence/agent-live-correction/13-completed-agent.png)
- [14-agent-maximized-output.png](evidence/agent-live-correction/14-agent-maximized-output.png)

## quality-correction-002

Browser retains a duplicate heading, nested tabs and legacy card beneath shared panel tabs

P1. interaction. runtime-reproduced. Disposition `repair-or-quality`.

The Browser surface row is followed by a second Browser title, a page-tab strip and then the toolbar. Repeated identities, differently scoped plus controls and competing frames consume space and make the existing browser look unfinished.

Recommendation. Complete the shared surface model: give each page one clear identity and unify tab/control density; remove the redundant legacy Browser heading/card layer where the panel already provides it.

Acceptance check. Open one and two pages, switch to Files and back, and close the active page. Active identity and add/close scope remain clear, with a single deliberate panel hierarchy and aligned content edges.

[Full investigation and reproduction conditions](reports/quality-correction.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel.rs:636-645](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L636-L645)
- [crates/ferrite-cockpit/src/cockpit/panel/preview.rs:498-515](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L498-L515)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1355-1405](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1355-L1405)
- [crates/ferrite-pane/src/preview.rs:641-672](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L641-L672)
- [docs/adr/0039-the-panel-is-one-tabbed-surface-host.md:1-24](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/adr/0039-the-panel-is-one-tabbed-surface-host.md#L1-L24)
- [apps/web/src/components/RightPanelTabs.tsx:599-641](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L599-L641)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:94-95](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L94-L95)

Review note. Chrome observed in headless screenshots and personally in installed Ferrite with a loaded example.com page. The unavailable headless page body is expected and is not counted as a native browser-load defect. This is a visual-quality judgment supported by actual assembly and ADR 0039.

Runtime evidence:

- [visual-quality-correction.md](reports/visual-quality-correction.md)
- [13-browser-after-files.png](evidence/panel-correction/13-browser-after-files.png)
- [15-browser-after-changes.png](evidence/panel-correction/15-browser-after-changes.png)

## quote-attachments-001

Cite has no reachable per-quote comment editor

P1. missing-capability. runtime-reproduced. Disposition `repair-or-quality`.

The user cannot attach their own words to an individual citation. Multiple citations cannot each receive an independently editable instruction. The ordinary composer remains a separate place to type general text; this finding does not establish that all typing after Cite is broken.

Recommendation. Add a native comment editor to the citation interaction, with Save, Cancel, multiline input, a focus return target and a later Edit comment action. Keep quote source and user comment separate. Correct composer.md's positive claim that reply cards already provide a usable commenting workflow.

Acceptance check. Using only UI actions, cite two different passages, add different multiline comments, edit one, cancel another edit, add general prompt text, reopen the draft and send. Each comment remains editable and paired with its quote. IME Enter does not save or send prematurely. A quote with no comment also sends.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/transcript/reply.rs:206-220](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/reply.rs#L206-L220)
- [crates/ferrite-pane/src/transcript/message_menu.rs:168-171](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L168-L171)
- [crates/ferrite-pane/src/attachments.rs:551-618](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L551-L618)
- [crates/ferrite-pane/src/composer.rs:409-419](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L409-L419)
- [crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:130-133](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/terminal.rs#L130-L133)
- [crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:954-975](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs#L954-L975)
- [crates/ferrite-cockpit/src/cockpit/visual_reference/feature_scenes.rs:560-561](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/visual_reference/feature_scenes.rs#L560-L561)
- [apps/web/src/components/chat/AssistantCitationChip.tsx:275](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L275)
- [apps/web/src/components/chat/AssistantCitationCommentEditor.tsx:81](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L81)
- [apps/web/src/components/ComposerPromptEditorTiptap.tsx:455](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L455)
- [apps/web/src/components/chat/ChatComposer.tsx:2989](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatComposer.tsx#L2989)

Review note. The user explicitly requires adding their own text to a citation. This is a required workflow gap, not an optional feature candidate. Clean pointer Cite and ordinary composer annotation worked, sending exactly > amber followed by My clean annotation. Clicking the quote tile exposed no editor; typing there was unhandled. This is a per-quote authoring gap, not proof that ordinary text cannot accompany a quote.

Runtime evidence:

- [runtime-interactions.md](reports/runtime-interactions.md)
- [21-clean-cite-and-annotation.png](evidence/headless/21-clean-cite-and-annotation.png)
- [22-clean-tile-no-editor.png](evidence/headless/22-clean-tile-no-editor.png)
- [provider-host.jsonl](evidence/headless/provider-host.jsonl)

## quote-attachments-007

Quoted file and skill syntax can become active provider input

P1. defect. source-proven. Disposition `repair-or-quality`.

Citing reference text can attach a local file or activate a Codex skill even though the user only meant to discuss the quoted text. A quote can therefore change the provider input beyond its visible citation card.

Recommendation. Carry typed context to provider adapters and expand quotation text only as text. At minimum isolate quoted material from attachment and skill scanning. Use the annotation encoder's explicit separation as an existing design precedent, not a blanket text replacement that corrupts quoted content.

Acceptance check. Against a fake provider and harmless fixture files, cite exact file/skill syntax and inspect the wire payload. It contains the unchanged reference text but no unsolicited file bytes, localImage, localAudio, mention or skill item. Explicitly attaching the same fixture still produces the intended typed item.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/quote.rs:34-47](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L34-L47)
- [crates/ferrite-core/src/prompt_files.rs:49-93](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/prompt_files.rs#L49-L93)
- [crates/ferrite-core/src/providers/claude/wire.rs:25-62](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/claude/wire.rs#L25-L62)
- [crates/ferrite-core/src/providers/codex/wire.rs:1242-1293](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex/wire.rs#L1242-L1293)
- [crates/ferrite-core/src/browser_annotations.rs:453-461](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser_annotations.rs#L453-L461)
- [packages/shared/src/assistantCitations.ts:148](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L148)
- [apps/web/src/components/chat/useComposerSend.ts:1110](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1110)
- [apps/web/src/components/chat/useComposerSend.ts:1292](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1292)

## quote-attachments-012

The reported pointer Cite regression still needs installed-app reproduction

P1. verification-gap. unverified. Disposition `verification-gap`.

The user reports that citing no longer works. Existing event handlers and fixture tests do not establish that the installed app accepts the real selection and reaches the composer.

Recommendation. Parent should capture the installed build identity, selection, toolbar, click result and focus destination. Keep the report open until that reproduction distinguishes no offer, dead activation, wrong destination and inability to type. Do not mark the regression fixed because a handler exists.

Acceptance check. On the installed build, test plain prose, code, tables, links, double/triple click, cross-block selection, scrolling, a narrow pane, right-panel focus and two open threads. Verify the citation reaches the intended draft and typing works. Record pointer coordinates and before/after evidence for every failure. Do not send to a live provider without an appropriate disposable thread.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/transcript/reply.rs:65-95](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/reply.rs#L65-L95)
- [crates/ferrite-cockpit/src/cockpit.rs:2127-2134](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L2127-L2134)
- [crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:512-615](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs#L512-L615)
- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:185](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L185)
- [apps/web/src/components/chat/ChatComposer.tsx:2544](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatComposer.tsx#L2544)

Review note. The isolated pinned-source build did not reproduce the blanket pointer Cite failure. A clean selection, Cite, ordinary text entry and send succeeded. The installed intermittent symptom remains unresolved, and this verification-gap record stays unverified.

Runtime evidence:

- [runtime-interactions.md](reports/runtime-interactions.md)
- [20-cite-clean-selection.png](evidence/headless/20-cite-clean-selection.png)
- [21-clean-cite-and-annotation.png](evidence/headless/21-clean-cite-and-annotation.png)
- [23-quote-sent.png](evidence/headless/23-quote-sent.png)
- [provider-host.jsonl](evidence/headless/provider-host.jsonl)

## subagents-background-02

Nested agents and immediate-parent navigation are missing from the normal drill path

P1. partial-capability. source-proven. Disposition `repair-or-quality`.

The core knows grandchildren, but inspecting a child loses the routes needed to inspect its children or identify the immediate parent.

Recommendation. Pass child mappings into agent transcripts, route OpenAgent from a Drill to another Drill, and expose immediate-parent and root navigation using AgentInfo.parent. Add a relationship list independent of retained spawn rows.

Acceptance check. Create Main > A > B with both providers. Open A from Main, then B from A. B identifies A as parent, opens A, and offers Main separately. Completed children remain reachable after spawn rows are outside the rendered window.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/side_chat.rs:477-482](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L477-L482)
- [crates/ferrite-cockpit/src/cockpit/side_chat.rs:527](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L527)
- [crates/ferrite-cockpit/src/cockpit/side_chat.rs:564](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L564)
- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:125-143](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L125-L143)
- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-225](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L203-L225)
- [crates/ferrite-cockpit/src/cockpit/subagents.rs:463-490](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L463-L490)
- [crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:49-96](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/transcript_glue.rs#L49-L96)
- [crates/ferrite-core/src/activity.rs:69-76](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/activity.rs#L69-L76)
- [apps/web/src/components/chat/ThreadRelationshipsControl.tsx:514-583](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L514-L583)
- [apps/web/src/components/chat/ThreadRelationshipsControl.tsx:784-859](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L784-L859)
- [apps/web/src/components/chat/ProviderSubagentBar.tsx:70-75](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L70-L75)

Review note. Controlled Main > Alpha > Grandchild rendered nested controls; both nested row and View live output did nothing. This verifies the normal drill failure, not every parent/ancestry/provider path.

Runtime evidence:

- [agent-live-correction.md](reports/agent-live-correction.md)
- [07-nested-row-click.png](evidence/agent-live-correction/07-nested-row-click.png)
- [08-nested-live-output-click.png](evidence/agent-live-correction/08-nested-live-output-click.png)

## subagents-background-03

Reopen closed tab restores empty Agent and SideChat tabs

P1. defect. runtime-reproduced. Disposition `repair-or-quality`.

The advertised reopen action returns a blank subagent view.

Recommendation. Reconstruct the agent resource through open_agent_drill when reopening its SurfaceId, or exclude it from reopen until a resource-aware restore exists.

Acceptance check. Open an agent with visible text, close its tab, then invoke Reopen closed tab. The same agent and transcript return. Repeat through Close all and confirm each restored tab has content.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel.rs:290-293](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L290-L293)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:313-319](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L313-L319)
- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:113-121](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L113-L121)
- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:48-52](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L48-L52)
- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:232-235](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L232-L235)
- [crates/ferrite-cockpit/src/cockpit/palette.rs:1294-1297](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/palette.rs#L1294-L1297)
- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-206](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L203-L206)
- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:48-53](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L48-L53)
- [crates/ferrite-cockpit/src/cockpit/side_chat.rs:308-324](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L308-L324)
- [crates/ferrite-cockpit/src/cockpit/palette.rs:1294-1298](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/palette.rs#L1294-L1298)

Related record [reconciliation-001](reports/reconciliation.md). Reopening a closed SideChat restores its outer tab but not its released conversation record, leaving an empty body instead of the previous work.

Additional acceptance check. Create a side conversation containing a unique marker through the UI, close its tab, and invoke Reopen closed tab. The same identity and marker return with working controls, or a deliberately unsupported action is unavailable and explained. Repeat during fork startup and failed restoration; no duplicate provider work or blank body is allowed.

Review note. Both paths restore a tab identifier after releasing its backing view. File previews are excluded because their renderer hydrates them on reopen. Both Agent and SideChat close/reopen failures were exercised through real controls. Reopen returns the tab identity with no backing body. File previews remain excluded.

Runtime evidence:

- [agent-live-correction.md](reports/agent-live-correction.md)
- [12-reopened-agent-blank.png](evidence/agent-live-correction/12-reopened-agent-blank.png)
- [draft-lifecycle-runtime-correction.md](reports/draft-lifecycle-runtime-correction.md)
- [18-sidechat-reopened.png](evidence/draft-lifecycle-correction/18-sidechat-reopened.png)
- [19-sidechat-reopened-settled.png](evidence/draft-lifecycle-correction/19-sidechat-reopened-settled.png)

## BROWSER-03

Right-click has no browser context menu

P2. interaction. source-proven. Disposition `repair-or-quality`.

Operators cannot discover normal link, image, selection, and editable-field actions through right-click.

Recommendation. Introduce a browser-specific context menu using native hit-test details. Do not apply arbitrary website menus to sealed content.

Acceptance check. Right-click a link and copy its destination; right-click selected page text and copy it; paste into an input through the menu. Each action operates on the clicked page.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-pane/src/visual/web/cef/handlers.rs:589-603](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L589-L603)
- [crates/ferrite-pane/src/visual/element.rs:643-689](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L643-L689)
- [crates/ferrite-pane/src/visual/element.rs:713-719](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L713-L719)

## BROWSER-04

Browser tabs discard page titles and show only hostnames

P2. interaction. source-proven. Disposition `repair-or-quality`.

Two pages on the same host, including two localhost apps on different ports, have indistinguishable tab labels. Favicons are absent.

Recommendation. Carry page identity metadata through service state and render title, favicon, and a full-address tooltip. Keep port in the fallback.

Acceptance check. Tabs titled Docs and Dashboard on one origin remain distinguishable; two untitled localhost ports show distinct fallback labels. Title changes update the correct tab.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:1355-1388](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1355-L1388)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:744-763](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L744-L763)
- [crates/ferrite-core/src/browser.rs:44-56](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser.rs#L44-L56)
- [crates/ferrite-pane/src/visual/web/mod.rs:69-77](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/mod.rs#L69-L77)
- [crates/ferrite-pane/src/visual/web/cef/handlers.rs:320-321](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L320-L321)
- [apps/web/src/components/RightPanelTabs.tsx:599-619](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L599-L619)
- [apps/web/src/components/RightPanelTabs.tsx:631-641](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L631-L641)

## BROWSER-05

Address entry rejects normal schemeless hostnames

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Common development addresses such as localhost:3000 require manual http:// entry.

Recommendation. Normalize human address input separately from the service authorization policy. Do not weaken scheme restrictions or silently turn arbitrary text into a network destination.

Acceptance check. localhost:3000 opens http://localhost:3000; example.com opens https://example.com. Explicit unsupported schemes still produce clear errors.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:1304-1323](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1304-L1323)
- [crates/ferrite-core/src/browser.rs:817-829](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser.rs#L817-L829)
- [packages/shared/src/preview.ts:183-218](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/preview.ts#L183-L218)
- [apps/web/src/components/preview/PreviewView.tsx:310-327](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L310-L327)

## BROWSER-06

Page navigation can overwrite an unfinished address edit

P2. defect. source-proven. Disposition `repair-or-quality`.

A redirect or same-page URL change can replace the address the user is currently typing.

Recommendation. Introduce explicit address editing state. Show committed URL while idle; preserve and validate the draft while editing; Escape should restore the committed URL.

Acceptance check. Type a new URL during a delayed redirect and a history.pushState update. The draft survives both. Escape restores the final page URL, and Enter navigates to the draft.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:667-672](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L667-L672)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:180-186](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L180-L186)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1304-1323](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1304-L1323)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:79-88](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L79-L88)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:157-180](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L157-L180)

## BROWSER-07

New-tab mode leaves navigation controls pointed at the hidden old page

P2. defect. source-proven. Disposition `repair-or-quality`.

Clicking Reload or Back while an empty new tab is shown unexpectedly acts on the previous tab and abandons new-tab entry.

Recommendation. Model the displayed target explicitly or consistently derive toolbar state from the displayed page. Keep draft new-tab state independent of the previous document.

Acceptance check. After clicking plus, Back, Forward, and Reload cannot affect the old page. The address draft remains until explicit submission, cancel, or tab selection.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:275-276](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L275-L276)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1407-1445](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1407-L1445)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1479-1481](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1479-L1481)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1505-1515](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1505-L1515)

## BROWSER-08

Loading changes browser geometry and shifts the address field

P2. interaction. source-proven. Disposition `repair-or-quality`.

Loading moves the page and changes its actual viewport size. The Annotate button also appears and disappears, changing address width.

Recommendation. Reserve one compact navigation row; keep annotation visible but disabled during loading and place progress in an overlay or fixed-height track.

Acceptance check. Record the page origin, page viewport size, and address bounds before and during a slow navigation. All remain stable, and loading remains visible. Narrow-width controls stay operable.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:1452-1466](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1452-L1466)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1521-1530](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1521-L1530)
- [crates/ferrite-pane/src/visual/element.rs:525-532](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L525-L532)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1521-1531](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1521-L1531)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1491-1497](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1491-L1497)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:94-95](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L94-L95)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:132-147](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L132-L147)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:295-300](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L295-L300)
- [apps/web/src/index.css:2424](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/index.css#L2424)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:196](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L196)

Related record [motion-010](reports/motion.md). A loading page adds and removes a plain text row rather than showing persistent progress feedback in the browser chrome.

Additional acceptance check. Navigate a deliberately slow page, then a fast page, failure, reload and back/forward. Loading feedback must remain visible, finish once, and not shift the page bounds. Under reduced motion status must remain legible without travel.

Review note. Parent native observations corroborated the loading branch: Annotate disappeared, Address shifted left and a Loading row appeared above the page. The installed commit is unknown and captures are conversation-only. The source finding remains source-proven; no timing, focus, controlled slow-load or network-error sequence was certified.

Runtime evidence:

- [native-correction.md](evidence/native-correction.md)

## BROWSER-09

Viewport size is inseparable from panel size

P2. missing-capability. source-proven. Disposition `capability-candidate`.

The operator cannot verify a site at a precise mobile or desktop viewport, rotate a preset, or compare a large layout while keeping the chat readable.

Recommendation. Add a user-selected viewport mode distinct from host size and device pixel ratio, plus explicit page zoom. Validate dimensions and preserve selection per browser tab.

Acceptance check. A 390 by 844 preset reports those CSS dimensions in the page while the right panel is resized; rotate swaps dimensions; Fill restores host-sized layout. Zoom has a visible reset.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-pane/src/visual/element.rs:504-505](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L504-L505)
- [crates/ferrite-pane/src/visual/element.rs:525-532](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L525-L532)
- [crates/ferrite-core/src/browser.rs:77-101](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser.rs#L77-L101)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1406-1478](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1406-L1478)
- [apps/web/src/browser/BrowserDeviceToolbar.tsx:29-84](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/BrowserDeviceToolbar.tsx#L29-L84)
- [apps/web/src/components/preview/PreviewView.tsx:1049-1054](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L1049-L1054)
- [apps/web/src/components/preview/PreviewMoreMenu.tsx:112-173](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewMoreMenu.tsx#L112-L173)

## BROWSER-10

Find, DevTools, hard reload, and page appearance controls are absent

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Long-page search and normal web development inspection require leaving the embedded browser. Cache bypass and theme checks have no operator controls.

Recommendation. Add typed page operations and a browser menu. Keep find scoped to the website rather than transcript search; surface supported runtime distinctions.

Acceptance check. Find highlights matches and cycles next/previous in the current page; DevTools targets the correct tab; hard reload bypasses a fixture cache; light/dark setting updates a media-query fixture.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-core/src/browser.rs:77-101](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser.rs#L77-L101)
- [crates/ferrite-pane/src/visual/web/mod.rs:82-103](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/mod.rs#L82-L103)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1406-1478](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1406-L1478)
- [apps/web/src/components/preview/PreviewMoreMenu.tsx:90-128](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewMoreMenu.tsx#L90-L128)
- [apps/web/src/components/preview/PreviewView.tsx:494-532](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L494-L532)
- [apps/web/src/components/preview/PreviewFindBar.tsx:1-40](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewFindBar.tsx#L1-L40)

## BROWSER-11

Inactive-page reclamation restores only a URL, losing navigation and page position

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A clean tab can lose its Back/Forward stack, scroll position, and transient UI state merely because it was hidden. Restart also restores only URLs and selection.

Recommendation. Preserve native tabs for normal switching or retain a navigation/session snapshot before reclaiming. Define the resource policy explicitly instead of immediate hidden-state disposal. Do not claim full T3 session restoration without checking its host lifetime.

Acceptance check. Visit A then B, scroll B, switch away and back through actual reclamation, and verify Back returns to A and B restores its scroll. Repeat after restart according to the documented persistence contract.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:1222-1259](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1222-L1259)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1284-1294](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1284-L1294)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:708-715](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L708-L715)
- [crates/ferrite-pane/src/visual/web/cef.rs:1397-1433](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef.rs#L1397-L1433)
- [crates/ferrite-core/src/browser.rs:728-785](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser.rs#L728-L785)

## BROWSER-12

An empty browser provides neither useful guidance nor recent/local destinations

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Opening the browser or a new tab leaves the user to recover a URL manually from terminal output or memory.

Recommendation. Render an actual empty/new-tab page with address guidance, scoped recent destinations, and discovered local servers. Reuse existing terminal/server discovery if available rather than introducing another scanner.

Acceptance check. First open and last-tab close show guidance. Starting a local development server yields a clearly labeled destination, and a recently visited URL can be reopened and removed.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:195-196](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L195-L196)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1338-1342](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1338-L1342)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1479-1545](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1479-L1545)
- [crates/ferrite-cockpit/src/cockpit/browser/annotations.rs:563-570](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser/annotations.rs#L563-L570)
- [crates/ferrite-core/src/browser.rs:728-745](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser.rs#L728-L745)
- [apps/web/src/components/preview/PreviewEmptyState.tsx:29-45](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewEmptyState.tsx#L29-L45)
- [apps/web/src/components/preview/PreviewEmptyState.tsx:50-89](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewEmptyState.tsx#L50-L89)

## BROWSER-13

Persistent website profiles have no operator management controls

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Cookies survive sessions, but the operator cannot clear broken authentication/cache or choose isolated browser identities from the browser UI.

Recommendation. First add visible active-profile identity and scoped clear-cookie/cache operations. Treat multi-profile, private sessions, and cookie import as separate product choices; do not import credentials without an explicit user action.

Acceptance check. A fixture cookie persists normally, clears through the named profile action, and stays isolated from another chosen profile if multi-profile support is adopted. The UI states which data will be cleared.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-pane/src/visual/web/cef/profile.rs:17-28](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/profile.rs#L17-L28)
- [crates/ferrite-pane/src/visual/web/cef/profile.rs:55-77](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/profile.rs#L55-L77)
- [crates/ferrite-core/src/browser.rs:77-101](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser.rs#L77-L101)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1406-1478](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1406-L1478)
- [apps/web/src/components/preview/PreviewMoreMenu.tsx:175-182](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewMoreMenu.tsx#L175-L182)
- [apps/web/src/components/preview/PreviewView.tsx:521-528](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L521-L528)
- [apps/web/src/components/settings/IntegrationsSettings.tsx:1111-1185](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/IntegrationsSettings.tsx#L1111-L1185)
- [apps/web/src/components/settings/BrowserImportWizard.tsx:464](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/BrowserImportWizard.tsx#L464)

## BROWSER-14

Browser capture is annotation-oriented and lacks direct recording/export affordances

P2. missing-capability. source-proven. Disposition `capability-candidate`.

An operator cannot directly record a browser interaction or capture a plain screenshot through the browser chrome for external sharing.

Recommendation. Reuse the existing capture pipeline for a plain screenshot command with output feedback. Decide recording scope separately; show active recording and its destination rather than hiding it behind a modifier-only gesture.

Acceptance check. A direct screenshot contains the current page at the chosen scale and can be opened from the reported destination. If recording is added, start/stop yields a playable file with a visible active-state indicator.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:1452-1463](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1452-L1463)
- [crates/ferrite-cockpit/src/cockpit/browser/annotations.rs:556-627](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser/annotations.rs#L556-L627)
- [crates/ferrite-core/src/browser.rs:77-101](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser.rs#L77-L101)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:244-265](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L244-L265)
- [apps/web/src/components/preview/PreviewView.tsx:549-780](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L549-L780)

## BROWSER-16

No browser-chrome escape to the system browser

P2. missing-capability. source-proven. Disposition `capability-candidate`.

When a website needs unsupported authentication, downloads, permissions, or development tools, the operator must manually copy the address and leave the app.

Recommendation. Add Open in system browser for the committed page URL, with an error if launching fails. Do not send the unsubmitted address draft.

Acceptance check. The action opens the current page in the configured browser; an unfinished address draft does not change the launched destination.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:1406-1478](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1406-L1478)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1490-1519](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1490-L1519)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:193-213](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L193-L213)
- [apps/web/src/components/preview/PreviewView.tsx:437-440](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L437-L440)

## composer-04

Queued Edit removes the message and merges it into unrelated draft text

P2. partial-capability. runtime-reproduced. Disposition `repair-or-quality`.

Editing a queued message changes queue membership immediately and combines it with a separate draft. There is no save-in-place or cancel that restores both original pieces of work.

Recommendation. Either rename this operation Take back and disclose its merge behavior, or add transactional queued editing with independent draft storage and a provider/server edit operation.

Acceptance check. With queued A/B and draft C, edit A and cancel: A/B order and C remain unchanged. Repeat and save A2: only A changes, C is restored, and no extra turn is started. Cover A starting while its edit is open.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:8098-8123](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L8098-L8123)
- [crates/ferrite-core/src/cockpit.rs:2388-2390](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L2388-L2390)
- [crates/ferrite-core/src/engine.rs:3883-3886](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L3883-L3886)
- [crates/ferrite-cockpit/src/cockpit.rs:2337-2358](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L2337-L2358)
- [crates/ferrite-pane/src/pane/composer.rs:925-928](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L925-L928)
- [apps/web/src/components/chat/useQueuedRunEdit.ts:162-184](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useQueuedRunEdit.ts#L162-L184)

Review note. Real queued-edit click canceled the queued UUID and combined QUEUED_SENTINEL with UNRELATED_DRAFT. Both texts survived; their independent identities did not. Multi-item and race branches remain untested.

Runtime evidence:

- [draft-lifecycle-runtime-correction.md](reports/draft-lifecycle-runtime-correction.md)
- [08-queue-before-edit-valid.png](evidence/draft-lifecycle-correction/08-queue-before-edit-valid.png)
- [09-queue-after-edit.png](evidence/draft-lifecycle-correction/09-queue-after-edit.png)
- [drafts-after-queue-edit.json](evidence/draft-lifecycle-correction/drafts-after-queue-edit.json)

## composer-05

The composer cannot choose queue versus steer per message

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users cannot deliberately defer a follow-up until the current turn finishes. A message intended as later work can alter the running turn.

Recommendation. Add an explicit dispatch intent through UI, protocol and provider adapters. Keep a local/default behavior preference with a one-shot alternate action.

Acceptance check. In both providers, Queue leaves the running turn unmodified and starts only after it ends; Steer reaches the running turn. Pointer tooltip, modifier-click, keyboard and accessibility label describe the same action. Unsupported steer never appears available.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:5585-5596](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L5585-L5596)
- [crates/ferrite-core/src/engine.rs:3706-3748](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L3706-L3748)
- [crates/ferrite-core/src/providers/codex.rs:604-621](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex.rs#L604-L621)
- [crates/ferrite-core/src/providers/claude.rs:683-699](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/claude.rs#L683-L699)
- [apps/web/src/components/chat/ComposerPrimaryActions.tsx:250-265](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L250-L265)
- [packages/client-runtime/src/state/composerDispatch.ts:1-51](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/client-runtime/src/state/composerDispatch.ts#L1-L51)
- [docs/user/composer.md:63-91](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L63-L91)

## composer-06

Stop deliberately allows queued work to continue

P2. intentional-difference. source-proven. Disposition `product-choice`.

A user expecting Stop to stop automation can see queued work start immediately. This is intentional current code behavior, not an accidental missing interrupt.

Recommendation. Make stopping semantics an explicit product decision. Prefer a hold state that survives restart, with Resume and separately named Send now behavior.

Acceptance check. With pending A/B, Stop visibly acknowledges interruption and neither A nor B runs until Resume. If retaining current semantics, the label and confirmation-free explanatory text must clearly state that queued work will continue.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:8269-8284](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L8269-L8284)
- [crates/ferrite-core/src/engine.rs:2660-2701](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L2660-L2701)
- [apps/web/src/components/chat/useComposerResume.ts:52-55](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerResume.ts#L52-L55)
- [docs/user/composer.md:246-271](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L246-L271)

## composer-07

Attachment readiness and native-image fallbacks are invisible

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A chip can imply that a model received an image even when it got only a path. Missing or oversized files have no composer warning. HEIC/HEIF is not a native image format in the provider attachment classifier.

Recommendation. Add provider-aware validation and explicit attachment state, including a visible file-reference fallback. Prepare images off the UI thread, and retain original bytes where needed.

Acceptance check. Each boundary case produces either a ready native image or a visible explanation before Send. Deleted/unreadable files block or require removal. HEIC is converted or clearly labeled as a file. The agent sees the expected image bytes for accepted inputs.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-pane/src/composer.rs:139-158](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L139-L158)
- [crates/ferrite-core/src/prompt_files.rs:96-105](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/prompt_files.rs#L96-L105)
- [crates/ferrite-core/src/providers/claude/wire.rs:23-55](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/claude/wire.rs#L23-L55)
- [crates/ferrite-core/src/providers/codex/wire.rs:1276-1293](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex/wire.rs#L1276-L1293)
- [apps/web/src/components/chat/useComposerAttachmentIntake.ts:247-284](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L247-L284)
- [apps/web/src/lib/attachmentUploadQueue.ts:135-161](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135-L161)
- [docs/user/composer.md:38-56](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L38-L56)

## composer-08

Large pastes stay inline with no composer length guard

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Large logs or documents fill the prompt directly and can hit provider or transport limits without an early actionable message. No performance regression is claimed without measurement.

Recommendation. Add a provider-aware send limit and optional large-paste-to-file path, retaining an editable override. Do not copy T3 numeric limits blindly.

Acceptance check. A large paste becomes a named readable text attachment with feedback; the override preserves inline text. Oversize sends do not clear content and explain how to proceed. Test Unicode byte/character boundaries and selection replacement.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-pane/src/composer.rs:594-619](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L594-L619)
- [crates/ferrite-cockpit/src/cockpit.rs:5552-5596](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L5552-L5596)
- [crates/ferrite-pane/src/line.rs:13-32](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/line.rs#L13-L32)
- [apps/web/src/components/chat/useComposerAttachmentIntake.ts:371-445](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L371-L445)
- [apps/web/src/components/chat/ComposerPromptLengthValidation.tsx:1-13](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPromptLengthValidation.tsx#L1-L13)
- [docs/user/composer.md:6-13](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L6-L13)

## composer-09

Context is mostly a shelf or serialized trailer rather than inline records

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Users cannot place an image, quote, review note or browser annotation at a precise point in instructions, move it as one inline object, or copy the object with its context to another draft.

Recommendation. Introduce structured context records while keeping a plain-text fallback for providers. Preserve positions and clipboard metadata without requiring every attachment to become a new visual component.

Acceptance check. Write instructions around two image chips and a quote; cut/paste them into a second draft and restart. Each preview, source and relative position survives, and the outgoing prompt identifies the intended object correctly.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-pane/src/composer.rs:139-166](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L139-L166)
- [crates/ferrite-core/src/prompt_files.rs:15-24](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/prompt_files.rs#L15-L24)
- [crates/ferrite-reviewr/src/ferrite.rs:538-547](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/ferrite.rs#L538-L547)
- [crates/ferrite-pane/src/attachments.rs:89-127](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L89-L127)
- [apps/web/src/composerDraftStore.ts:2600-2621](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2600-L2621)
- [packages/contracts/src/composerContext.ts:1-180](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/composerContext.ts#L1-L180)
- [docs/user/composer.md:323-379](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L323-L379)

## composer-10

The mention menu has no PR, thread or model-reference search

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users must manually find and describe another thread, PR or model. Structured references that an agent can inspect on demand are missing from the composer.

Recommendation. Add reference-specific item types and payloads. Reuse PR/thread identity from their owning modules rather than scraping displayed text. Keep linked-machine transport separate from local thread references.

Acceptance check. Pick a PR by number and text, a thread by title, and a model by alias. Each chip previews the right source; model mention leaves active model unchanged; the agent receives a resolvable reference. A sidebar thread drop has the same result as a menu pick.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:6288-6307](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L6288-L6307)
- [crates/ferrite-pane/src/composer.rs:1142-1155](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L1142-L1155)
- [crates/ferrite-pane/src/prompt_drop.rs:11-45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/prompt_drop.rs#L11-L45)
- [apps/web/src/components/chat/useComposerMenuItems.ts:60-79](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L60-L79)
- [apps/web/src/components/chat/composerModelMentions.ts:1-43](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/composerModelMentions.ts#L1-L43)
- [packages/client-runtime/src/composerThreadItems.ts:1-115](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/client-runtime/src/composerThreadItems.ts#L1-L115)
- [docs/user/composer.md:332-360](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L332-L360)

## composer-11

Review comment drafts lose their structured controls after reopen

P2. defect. source-proven. Disposition `repair-or-quality`.

Review comments survive as prose but no longer have their original review attachment controls after the pane is recreated. This is loss of editability and source structure, not loss of the comment words.

Recommendation. Persist comments separately from outbound prompt text and hydrate them with the other draft context records.

Acceptance check. Add two anchored review comments, restart, and remove just one through its original control. The remaining comment retains its file/line anchor and sends once; neither appears as duplicated raw export text.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/drafts.rs:28-56](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/drafts.rs#L28-L56)
- [crates/ferrite-pane/src/composer.rs:494-513](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L494-L513)
- [crates/ferrite-reviewr/src/ferrite.rs:538-547](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/ferrite.rs#L538-L547)
- [crates/ferrite-pane/src/composer.rs:354-369](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L354-L369)
- [apps/web/src/composerDraftStore.ts:2600-2621](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2600-L2621)

## coverage-quality-02

Some UI tests bypass real keyboard and opener dispatch

P2. verification-gap. source-proven. Disposition `verification-gap`.

Direct handler calls can pass when binding precedence, focus or the visible opener prevents the user reaching that handler.

Recommendation. Add real click/key regression paths with production bindings, normal viewport and a populated composer. Keep helper tests but do not count them as entry-point verification.

Acceptance check. Empty-panel Enter opens its highlighted panel without sending a prompt. Each PR entry point visibly opens its intended target through normal event dispatch.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tests/tools.rs:190-220](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/tools.rs#L190-L220)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:687-691](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L687-L691)
- [crates/ferrite-cockpit/src/cockpit/tests/pr.rs:201-247](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/pr.rs#L201-L247)
- [crates/ferrite-cockpit/src/cockpit/tests/pr.rs:92-134](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/pr.rs#L92-L134)
- [scripts/scenarios/add-project-tab-reaches-submit.e2e.ts:12-20](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/scripts/scenarios/add-project-tab-reaches-submit.e2e.ts#L12-L20)

## coverage-quality-03

Required-form editability coverage never proves valid typed submission

P2. verification-gap. source-proven. Disposition `verification-gap`.

The negative-only test can pass if typing never edits the required field, because both an invalid value and an untouched empty field are rejected.

Recommendation. Type a valid value through the real field, repaint, submit, assert the exact typed answer and unchanged composer.

Acceptance check. A typed 2 survives repaint and reaches the correct decision as integer 2; invalid 9 produces a visible error and no answer.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tests/provider_forms.rs:306-359](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/provider_forms.rs#L306-L359)
- [crates/ferrite-cockpit/src/cockpit/tests/provider_forms.rs:229-276](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/provider_forms.rs#L229-L276)
- [docs/audits/2026-09-14-full-ux/findings.md:50-68](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/audits/2026-09-14-full-ux/findings.md#L50-L68)

## coverage-quality-04

Default UI checks do not establish native Chromium behavior

P2. verification-gap. source-proven. Disposition `verification-gap`.

Synthetic browser frames and a non-CEF app build cannot prove native page input, focus transfer, navigation or frame delivery.

Recommendation. Use a feature-aware native smoke gate for browser changes against a local fixture page and temporary profile.

Acceptance check. A CEF-enabled app accepts a real page-input click, text and Enter; back/forward change the visible document; host focus returns correctly. Fake-engine tests cannot satisfy this gate.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [.github/workflows/ci.yml:3-7](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/.github/workflows/ci.yml#L3-L7)
- [crates/ferrite-pane/src/visual/web/mod.rs:14-29](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/mod.rs#L14-L29)
- [crates/ferrite-cockpit/Cargo.toml:11-23](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/Cargo.toml#L11-L23)

## coverage-quality-05

Window geometry still resets at launch

P2. interaction. source-proven. Disposition `repair-or-quality`.

Users must repeatedly resize and reposition Ferrite because launch always supplies centered 1440 by 900 bounds.

Recommendation. Persist normal bounds and maximized state, validate restored geometry against attached displays, and use a fitting fallback.

Acceptance check. Temporary-profile relaunch restores usable size, position and maximization; a disconnected saved monitor never strands the window.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [crates/ferrite/src/lib.rs:349-357](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite/src/lib.rs#L349-L357)
- [docs/audits/2026-09-14-full-ux/findings.md:711-724](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/audits/2026-09-14-full-ux/findings.md#L711-L724)
- [apps/desktop/src/window/DesktopWindow.ts:452-470](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L452-L470)
- [apps/desktop/src/window/DesktopWindow.ts:535-565](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L535-L565)
- [apps/desktop/src/window/DesktopWindow.ts:983](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L983)

## coverage-quality-09

CLI help advertises variadic provider arguments that the parser rejects

P2. interaction. runtime-reproduced. Disposition `repair-or-quality`.

Parent reproduced a provider launch failure when following the apparent list syntax. Repeated --args flags work, but help does not make that invocation clear.

Recommendation. Print repeatable-list syntax and a provider-argument example in generated help, or explicitly define a trailing passthrough boundary. Correct callers relying on unsupported syntax.

Acceptance check. The generated thread-start help example parses to args ["--model", "fable", "--effort", "high"] without launching a provider; unknown Ferrite flags retain precise errors.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [crates/ferrite-core/src/api/cli.rs:448-469](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/api/cli.rs#L448-L469)
- [crates/ferrite-core/src/api/cli.rs:252-270](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/api/cli.rs#L252-L270)
- [crates/ferrite-core/src/api/tests.rs:1629-1656](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/api/tests.rs#L1629-L1656)
- [docs/audits/2026-11-13-t3-parity/evidence/runtime-observations.md:8](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/audits/2026-11-13-t3-parity/evidence/runtime-observations.md#L8)

## input-focus-005

No workspace-area focus cycle or per-surface focus restoration connects the composer and panel

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users who enter the terminal or panel cannot rely on one keyboard command to move among major areas. Opening or switching a panel tab does not establish a consistent focus handoff. Focus behavior depends on the specific body component.

Recommendation. Introduce explicit focus ownership for major visible areas and a remembered focus target per panel surface. Route keyboard focus deliberately on open, switch and close. Keep the composer's useful local tool walk, but provide a separate area-navigation command.

Acceptance check. F6 and Shift-F6, or documented equivalents, visit visible nav, conversation, composer and panel areas. They leave a terminal and browser without typing into them. Reopening a tab restores its last valid control. A closed or hidden tab never retains focus.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [crates/ferrite-ui/src/keymap.rs:263-310](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L263-L310)
- [crates/ferrite-cockpit/src/cockpit.rs:5796-5914](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L5796-L5914)
- [crates/ferrite-cockpit/src/cockpit.rs:10109-10274](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L10109-L10274)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:282-284](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L282-L284)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:550-620](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L550-L620)
- [packages/shared/src/keybindings.ts:59-63](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L59-L63)
- [apps/web/src/components/AppSidebarLayout.tsx:175-195](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/AppSidebarLayout.tsx#L175-L195)
- [apps/web/src/lib/workspaceFocus.ts:99-174](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/workspaceFocus.ts#L99-L174)
- [apps/web/src/lib/workspaceFocus.ts:175-257](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/workspaceFocus.ts#L175-L257)
- [apps/web/src/components/preview/PreviewPanelShell.tsx:112-148](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L112-L148)

## input-focus-006

Embedded browser input has no composition or native text-input bridge

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Users of input methods, composed text and native text services lack an implemented path equivalent to Ferrite's composer. Sending raw key_char values is not a composition protocol. Exact failures for each operating-system input method need runtime checks.

Recommendation. Implement native text-input and composition for the offscreen browser, including caret bounds and commit/cancel handling. Do not treat synthetic per-key CHAR events as sufficient. Audit the custom terminal input path separately because it also reads raw key events.

Acceptance check. Compose and commit a Japanese phrase and Chinese candidate sequence in a local browser textarea on each supported OS. Cancel a composition with Escape, move the candidate cursor, and confirm no host command runs and the candidate window follows the caret.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [crates/ferrite-pane/src/visual/element.rs:265-289](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L265-L289)
- [crates/ferrite-pane/src/visual/web/mod.rs:121-162](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/mod.rs#L121-L162)
- [crates/ferrite-pane/src/visual/web/cef/input.rs:60-110](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/input.rs#L60-L110)
- [crates/ferrite-pane/src/composer.rs:984-1021](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L984-L1021)
- [crates/ferrite-pane/src/composer.rs:1852-1854](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L1852-L1854)
- [apps/desktop/src/preview/Manager.ts:1540-1548](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L1540-L1548)

## input-focus-008

Panel resizing relies on root hover events that occluding children can block

P2. interaction. inferred. Disposition `repair-or-quality`.

A resize drag may stall over the panel edge or another occluding control and may miss its release. This is a source-backed risk, not a measured or runtime-reproduced failure.

Recommendation. Use an explicit captured drag session or capture-phase window listeners with an active-drag guard. End it on release, cancellation, window blur and owner disappearance. Confirm the actual failure before selecting the smallest repair.

Acceptance check. Resize continuously across the edge, tabs, browser and terminal. Release outside the original edge and outside the window. Width follows continuously and never changes after release. Include slow movement smaller than the edge width.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel.rs:480-484](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L480-L484)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:500-542](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L500-L542)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:586-604](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L586-L604)
- [crates/ferrite-cockpit/src/cockpit.rs:10506-10514](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L10506-L10514)
- [crates/ferrite-cockpit/src/cockpit.rs:10534-10549](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L10534-L10549)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:550-559](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L550-L559)
- [vendor/gpui-pre/src/elements/div.rs:215-228](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/div.rs#L215-L228)
- [vendor/gpui-pre/src/elements/div.rs:304-317](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/div.rs#L304-L317)
- [vendor/gpui-pre/src/window.rs:1187-1208](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/window.rs#L1187-L1208)
- [apps/web/src/hooks/useResizeDrag.ts:47-71](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useResizeDrag.ts#L47-L71)
- [apps/web/src/hooks/useResizeDrag.ts:82-125](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useResizeDrag.ts#L82-L125)

## input-focus-009

Find in a Files editor searches the conversation instead of the file

P2. defect. runtime-reproduced. Disposition `repair-or-quality`.

Users searching the code or prose currently visible in Files get a transcript search. The shortcut changes the wrong reading context and does not offer file-local matching.

Recommendation. Route Find through the focused surface and implement a file-local search owner for code and prose. Keep transcript search scoped to conversation focus.

Acceptance check. Use a file containing a sentinel absent from the transcript. Focus the file and invoke Find. The sentinel matches and scrolls into view in the file; the transcript search remains closed. Repeat for Markdown and after returning from another panel tab.

[Full investigation and reproduction conditions](reports/input-focus.md).

Evidence:

- [crates/ferrite-ui/src/keymap.rs:263-265](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L263-L265)
- [crates/ferrite-cockpit/src/cockpit.rs:6144-6164](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L6144-L6164)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:271-293](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L271-L293)
- [crates/ferrite-cockpit/src/cockpit/tools/editor.rs:426-504](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/editor.rs#L426-L504)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:663-687](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L663-L687)
- [apps/web/src/components/files/FilePreviewPanel.browser.test.tsx:189-225](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.browser.test.tsx#L189-L225)

Review note. Cmd+F after clicking the file editor opened transcript Find and highlighted a transcript match in the pinned headless build.

Runtime evidence:

- [runtime-interactions.md](reports/runtime-interactions.md)
- [07-file-find.png](evidence/headless/07-file-find.png)

## link-affordances-01

Transcript web-link menus cannot explicitly open the system browser

P2. missing-capability. runtime-reproduced. Disposition `capability-candidate`.

Users cannot choose their normal browser, authenticated profile or extensions from the link itself. The only Open action requests Ferrite Browser. Copying and manually pasting the address is the available escape.

Recommendation. Add an explicit system-browser verb to the shared link interaction, preserving the resolved address. Name browser destinations consistently. This is separate from BROWSER-16, which owns opening the already active browser page externally.

Acceptance check. Right-click [Example](https://example.com/a?b=c#d). Open in system browser launches that exact URL without creating or selecting a Ferrite tab. Open in integrated browser opens it in the owning thread. Copy link address copies the URL, not Example. Repeat while text is selected and while streaming.

[Full investigation and reproduction conditions](reports/link-affordances.md).

Evidence:

- [crates/ferrite-pane/src/transcript/message_menu.rs:67-84](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L67-L84)
- [crates/ferrite-pane/src/transcript/message_menu.rs:132-139](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L132-L139)
- [crates/ferrite-pane/src/transcript/message_menu.rs:192-198](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L192-L198)
- [crates/ferrite-pane/src/preview.rs:322-331](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L322-L331)
- [crates/ferrite-cockpit/src/cockpit/tests/link_clicks.rs:267-289](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/link_clicks.rs#L267-L289)
- [apps/web/src/components/chat/externalLinkContextMenu.ts:26-30](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/externalLinkContextMenu.ts#L26-L30)
- [apps/web/src/components/chat/externalLinkContextMenu.ts:111-124](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/externalLinkContextMenu.ts#L111-L124)
- [apps/web/src/components/ChatMarkdown.tsx:1299-1328](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1299-L1328)

Review note. The actual transcript menu showed Open link and Copy link, with no explicit system-browser action. The installed copy-only symptom was not reproduced on this fixture. No browser navigation outcome was tested.

Runtime evidence:

- [runtime-interactions.md](reports/runtime-interactions.md)
- [28-web-menu-actual.png](evidence/headless/28-web-menu-actual.png)

## link-affordances-02

Modified and middle clicks on transcript web links ignore destination intent

P2. interaction. source-proven. Disposition `repair-or-quality`.

Cmd/Ctrl-click and middle-click cannot bypass Ferrite Browser. The same gesture works differently on a sidebar PR badge, creating an unpredictable everyday navigation rule.

Recommendation. Resolve link activation once from target, button, modifiers and user preference. Keep the gesture distinct from browser-page target=_blank handling, already owned by BROWSER-01. T3 supports a saved app/system preference; Ferrite can choose a different default without ignoring overrides.

Acceptance check. Exercise plain, Cmd/Ctrl, Shift and middle clicks on the same transcript URL and sidebar PR. Assert the documented destination and whether selection changes. Dragging still selects without opening, and right-click never navigates.

[Full investigation and reproduction conditions](reports/link-affordances.md).

Evidence:

- [crates/ferrite-pane/src/rich.rs:605-645](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L605-L645)
- [vendor/gpui-base/src/text/inline.rs:1200-1241](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/inline.rs#L1200-L1241)
- [crates/ferrite-pane/src/preview.rs:324-330](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L324-L330)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:342-355](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L342-L355)
- [crates/ferrite-cockpit/src/cockpit.rs:12806-12819](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12806-L12819)
- [apps/web/src/browser/browserLinkTarget.ts:11-27](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/browserLinkTarget.ts#L11-L27)
- [apps/web/src/components/ChatMarkdown.tsx:1263-1297](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1263-L1297)
- [apps/desktop/src/window/DesktopWindow.ts:742-760](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L742-L760)

## link-affordances-03

Issue-reference context menus open and copy an internal scheme instead of the real URL

P2. defect. source-proven. Disposition `repair-or-quality`.

A generated #212 reference can work on left click but fail through Open link. Copy link yields issue:212 rather than a shareable repository issue URL.

Recommendation. Represent resolved target identity separately from display text and internal renderer schemes. Share decoding between clicks, menus and copy. Audit ferrite-path:, ferrite-thread:, ferrite-chip: and ferrite-skill: at the same boundary; do not advertise OS actions for app-only targets.

Acceptance check. For origin git@github.com:owner/repo.git and #212, plain click and both browser menu actions use https://github.com/owner/repo/issues/212; Copy link address copies that URL. Missing origin produces an unavailable reason rather than issuing issue:212 to the OS.

[Full investigation and reproduction conditions](reports/link-affordances.md).

Evidence:

- [crates/ferrite-pane/src/file_links.rs:607-614](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/file_links.rs#L607-L614)
- [crates/ferrite-pane/src/file_links.rs:653-664](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/file_links.rs#L653-L664)
- [crates/ferrite-pane/src/rich.rs:617-625](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L617-L625)
- [crates/ferrite-pane/src/transcript/message_menu.rs:278-282](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L278-L282)
- [crates/ferrite-pane/src/transcript/message_menu.rs:192-198](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L192-L198)
- [crates/ferrite-pane/src/file_links.rs:38-42](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/file_links.rs#L38-L42)
- [apps/web/src/components/ChatMarkdown.tsx:1192-1214](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1192-L1214)
- [apps/web/src/components/ChatMarkdown.tsx:1221-1228](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1221-L1228)
- [apps/web/src/components/ChatMarkdown.tsx:1312-1328](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1312-L1328)

## link-affordances-04

Custom file chips lose target-aware context actions

P2. partial-capability. runtime-reproduced. Disposition `repair-or-quality`.

An inline source-file chip supports opening but lacks reliable right-click actions for its full path, relative path, editor or file manager. A user cannot copy the actual destination from an abbreviated label with the confidence provided by a normal URL link.

Recommendation. Pass a typed file target through custom rendering and install its own context menu. Offer Copy full path and Copy relative path where meaningful, Open in Files at the referenced line, and supported editor/reveal actions. Keep display-label copying as text selection.

Acceptance check. Right-click a chip whose visible label differs from its path. Copy full/relative path returns the intended path with documented position suffix. Open in Files reveals line 42. Repeat after right-clicking an unrelated URL to ensure no stale target is reused, and for missing files and image chips.

[Full investigation and reproduction conditions](reports/link-affordances.md).

Evidence:

- [crates/ferrite-pane/src/rich.rs:567-575](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L567-L575)
- [vendor/gpui-base/src/text/inline_flow.rs:193-233](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/inline_flow.rs#L193-L233)
- [vendor/gpui-base/src/text/inline_flow.rs:470-481](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/inline_flow.rs#L470-L481)
- [crates/ferrite-pane/src/attachments.rs:795-855](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L795-L855)
- [crates/ferrite-pane/src/file_links.rs:354-380](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/file_links.rs#L354-L380)
- [crates/ferrite-pane/src/transcript/message_menu.rs:269-284](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L269-L284)
- [crates/ferrite-pane/src/transcript/message_menu.rs:199-203](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L199-L203)
- [apps/web/src/components/chat/MarkdownFileLink.tsx:247-309](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownFileLink.tsx#L247-L309)
- [apps/web/src/components/chat/MarkdownFileLink.logic.ts:32-49](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownFileLink.logic.ts#L32-L49)
- [apps/web/src/components/ChatMarkdown.tsx:835-840](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L835-L840)

Review note. Right-clicking the rendered sample.rs chip opened whole-message Copy, Attach, Pin and Fork actions, with no file-target actions.

Runtime evidence:

- [runtime-interactions.md](reports/runtime-interactions.md)
- [29-file-chip-context-menu.png](evidence/headless/29-file-chip-context-menu.png)

## link-affordances-05

Markdown section links are sent to the OS instead of scrolling within the document

P2. defect. source-proven. Disposition `repair-or-quality`.

Tables of contents and section references in generated explanations or plans do not navigate to their headings.

Recommendation. Add document-scoped heading identity and fragment navigation before file or external URL dispatch. Preserve a stable heading target across streaming updates and avoid selecting a heading in another answer with the same name.

Acceptance check. A local fixture with two answers containing Details headings scrolls to the clicked answer's heading. Percent-encoded fragments and duplicate heading suffixes work. Missing anchors give a defined result and never launch an external application.

[Full investigation and reproduction conditions](reports/link-affordances.md).

Evidence:

- [crates/ferrite-pane/src/rich.rs:628-644](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L628-L644)
- [crates/ferrite-pane/src/file_links.rs:25-28](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/file_links.rs#L25-L28)
- [crates/ferrite-pane/src/preview.rs:324-326](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L324-L326)
- [vendor/gpui-base/src/text/text_view.rs:95-112](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/text_view.rs#L95-L112)
- [apps/web/src/components/ChatMarkdown.tsx:1215-1233](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1215-L1233)
- [apps/web/src/components/chat/markdownFragments.ts:24-61](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/markdownFragments.ts#L24-L61)
- [apps/web/src/components/chat/markdownFragments.ts:83-105](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/markdownFragments.ts#L83-L105)

## link-affordances-06

Ordinary Markdown links and their transcript menus have no keyboard activation path

P2. interaction. source-proven. Disposition `repair-or-quality`.

Keyboard users cannot traverse and activate the same web links available to pointer users, or choose a link menu action after opening the pointer-only transcript menu.

Recommendation. Give inline links stable focusable semantic targets and reuse the app's accessible menu implementation. Keep this scoped to transcript links; input-focus-003 already owns right-panel control accessibility.

Acceptance check. Without a pointer, focus a link, activate it with Enter, open its context menu and choose Open in system browser or Copy address. Escape closes the menu and restores link focus. Re-rendering streaming text does not lose the focused target or submit the composer.

[Full investigation and reproduction conditions](reports/link-affordances.md).

Evidence:

- [vendor/gpui-base/src/text/inline.rs:1132-1241](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/inline.rs#L1132-L1241)
- [vendor/gpui-base/src/text/state.rs:39-49](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/state.rs#L39-L49)
- [vendor/gpui-base/src/text/text_view.rs:701-718](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/text_view.rs#L701-L718)
- [crates/ferrite-pane/src/rich.rs:605-613](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L605-L613)
- [crates/ferrite-pane/src/transcript/message_menu.rs:210-243](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L210-L243)
- [crates/ferrite-pane/src/link_chip.rs:119-134](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/link_chip.rs#L119-L134)
- [apps/web/src/components/ChatMarkdown.tsx:1221-1229](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1221-L1229)
- [apps/web/src/components/chat/MarkdownFileLink.tsx:329-377](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownFileLink.tsx#L329-L377)
- [apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx:69-85](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx#L69-L85)

Review note. Escape left the open link menu visible after real key dispatch. This proves the dismissal failure in the tested state; the broader keyboard-activation claim remains source-proven.

Runtime evidence:

- [runtime-interactions.md](reports/runtime-interactions.md)
- [30-web-menu-after-escape.png](evidence/headless/30-web-menu-after-escape.png)

## link-affordances-07

Right-clicking a sidebar PR badge can offer Copy link for the thread instead of the PR

P2. defect. source-proven. Disposition `repair-or-quality`.

A user who right-clicks an underlined PR number and selects Copy link can copy a Ferrite thread link rather than the forge PR address. The menu does not represent the clicked link.

Recommendation. Handle the PR badge context gesture and stop propagation after capturing its explicit PR target. Label any thread action Copy thread link. Existing pr-navigation findings own the primary route and visibility behavior; this finding only adds wrong clipboard/context identity.

Acceptance check. Right-click B's PR while A is focused. Copy PR link returns B's exact forge URL without changing focus. Right-click the remainder of B's row and Copy thread link returns B's Ferrite link. A missing PR URL disables URL actions with a reason.

[Full investigation and reproduction conditions](reports/link-affordances.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:12806-12819](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12806-L12819)
- [crates/ferrite-cockpit/src/cockpit/nav_list.rs:339-344](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_list.rs#L339-L344)
- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:457-462](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L457-L462)
- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:828-829](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L828-L829)
- [apps/web/src/components/pullRequest/PullRequestRow.tsx:143-154](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestRow.tsx#L143-L154)
- [apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx:69-76](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx#L69-L76)
- [apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts:17-23](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts#L17-L23)
- [apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts:39-45](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts#L39-L45)

## motion-001

Sidebar and right-panel geometry snap across structural changes

P2. motion. source-proven. Disposition `repair-or-quality`.

The entire workspace changes width without an intermediate visual state. This directly matches the user's snapping-sidebar complaint.

Recommendation. Give sidebar open and close a shared interruptible layout transition. Keep pointer resize immediate, skip restored layouts, retain a closing face until it finishes, and honor reduced motion. Replace the documented prohibition on sidebar motion with the user's requested policy.

Acceptance check. With motion enabled, record sidebar toggles from toolbar and shortcut. At least one intermediate width must appear, the board must follow without stale hit regions, and reversing before completion must continue from the displayed width. Reduced motion and restored startup layouts must settle immediately.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:2829-2838](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L2829-L2838)
- [crates/ferrite-ui/src/motion.rs:33](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L33)
- [crates/ferrite-ui/src/theme.rs:3531-3537](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/theme.rs#L3531-L3537)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:25-27](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L25-L27)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:399-420](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L399-L420)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:433-456](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L433-L456)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:550-620](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L550-L620)
- [apps/web/src/components/ui/sidebar.tsx:388](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/sidebar.tsx#L388)
- [apps/web/src/panelAnimations.ts:31](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L31)
- [packages/contracts/src/settings.ts:155](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/settings.ts#L155)
- [apps/web/src/components/preview/PreviewPanelShell.tsx:81](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L81)
- [apps/web/src/components/preview/PreviewPanelShell.tsx:169](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L169)
- [apps/web/src/panelAnimations.ts:41](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L41)

Related record [motion-002](reports/motion.md). Files, browser, terminal, PR, agents and other panel tools abruptly take or return workspace area.

Additional acceptance check. Open and close each panel kind, reverse mid-transition, switch to a wider kind, maximize, then drag its seam. Intermediate geometry must be visible only for discrete transitions, outgoing content must lose input ownership at once, and browser/terminal dimensions must stay correct.

Review note. Both shell width findings share the explicit snap policy. The detailed report retains separate contracts for collapse, reopen, maximize, and width floors. Panel sizing constraints remain a separate finding.

## motion-003

Most panel tab bodies have no transition despite the motion catalog

P2. motion. source-proven. Disposition `repair-or-quality`.

Tab changes replace large bodies instantly and differ from document previews. The catalog incorrectly promises broader coverage.

Recommendation. Give surface activation one transition owner keyed by thread and surface identity. Apply the agreed body transition to Overview, Background, Files, Changes, Terminal, SideChat, Agent and PR, and coordinate document preview fades to avoid double animation. Correct the catalog.

Acceptance check. Switch through every panel kind and PR subview using both mouse and keyboard. Confirm one visible transition per activation, no replay on unrelated updates, no outgoing input target, and no lost terminal/browser focus.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel.rs:657-694](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L657-L694)
- [crates/ferrite-cockpit/src/cockpit/panel/preview.rs:244-257](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L244-L257)
- [crates/ferrite-cockpit/src/cockpit/panel/preview.rs:444-449](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L444-L449)
- [crates/ferrite-ui/src/motion.rs:53](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L53)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:460-486](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L460-L486)
- [apps/web/src/components/preview/PreviewPanelShell.tsx:169](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L169)

## motion-005

Shelves and connector details reveal their bodies instantly

P2. motion. source-proven. Disposition `repair-or-quality`.

Expanding a shelf or connector section moves neighboring content without showing the relationship between the header and revealed rows.

Recommendation. Add measured, clipped body reveal for nonvirtualized shelves and connector sections. Treat virtualized transcript disclosures separately so visible bounds, scroll anchoring and hit testing agree throughout the transition. Keep chevron timing coordinated with body motion.

Acceptance check. Expand and collapse every shelf and Connectors, including rapid reversal and a long list. Rows must remain clipped to the displayed body, adjacent headings must move continuously, and reduced motion must reveal the final content immediately.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_list.rs:69-84](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_list.rs#L69-L84)
- [crates/ferrite-cockpit/src/nav/list.rs:114-117](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/nav/list.rs#L114-L117)
- [crates/ferrite-cockpit/src/cockpit/panel/sections.rs:249-278](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/sections.rs#L249-L278)
- [crates/ferrite-ui/src/motion.rs:57-70](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L57-L70)
- [apps/web/src/components/ui/collapsible.tsx:21](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/collapsible.tsx#L21)
- [apps/web/src/components/Sidebar.tsx:2040](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2040)
- [apps/web/src/components/chat/TimelineV2EventRow.tsx:145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineV2EventRow.tsx#L145)

## motion-006

System reduced-motion changes are not observed while Ferrite runs

P2. defect. source-proven. Disposition `repair-or-quality`.

A user who enables Reduce Motion after launch keeps the previous animation policy until restarting. Turning it off also is not observed.

Recommendation. Subscribe to platform accessibility preference changes and route them through set_reduce_motion. Resolve the initial preference before animated content is presented, or adopt a conservative initial state until it is known. Ensure active scroll and layout transitions land when the setting changes.

Acceptance check. While a loop, panel reveal and smooth scroll are active, toggle the OS preference without restarting. All animation must reach a static state promptly; toggling back must affect subsequent transitions. Repeat with the environment override.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-ui/src/motion.rs:2152-2177](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L2152-L2177)
- [vendor/gpui-pre/src/app.rs:864](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/app.rs#L864)
- [vendor/gpui-pre/src/app.rs:1065-1067](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/app.rs#L1065-L1067)
- [apps/web/src/hooks/useMediaQuery.ts:63](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useMediaQuery.ts#L63)
- [apps/web/src/panelAnimations.ts:35](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L35)

## motion-007

Non-macOS builds do not read the system reduced-motion preference

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Windows and other supported non-macOS users cannot rely on their system animation preference reaching Ferrite through this integration.

Recommendation. Implement the relevant platform setting bridge and change observation on supported operating systems. Preserve the explicit environment override for test and operator control.

Acceptance check. On each supported non-macOS platform, launch with the system animation setting disabled and then change it while running. Built-in transitions and continuous indicators must follow the setting without an environment variable.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-ui/src/motion.rs:2155-2165](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L2155-L2165)
- [vendor/gpui-pre/src/app.rs:864](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/app.rs#L864)
- [crates/ferrite-ui/src/motion.rs:2081-2082](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L2081-L2082)
- [apps/web/src/hooks/useMediaQuery.ts:63](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useMediaQuery.ts#L63)
- [apps/web/src/panelAnimations.ts:35](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L35)

## motion-008

Streaming tail follow still jumps with content growth

P2. motion. source-proven. Disposition `repair-or-quality`.

The answer can move in provider-update increments even though its new text fades in. Send scrolling and jump-to-end already animate, which makes continuous follow inconsistent.

Recommendation. Introduce a dedicated interruptible live-follow policy while work is running. Keep idle growth, history restoration and user-driven scroll separate. Preserve the reading anchor and stop following immediately on wheel, scrollbar or page navigation.

Acceptance check. With a long answer visible at the tail, stream uneven chunks and tool rows. Record intermediate scroll positions, then interrupt with wheel and scrollbar. The transcript must stop exactly where the user takes control and resume only when requested.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-pane/src/transcript/scroll.rs:78-80](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L78-L80)
- [crates/ferrite-pane/src/transcript/scroll.rs:111-114](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L111-L114)
- [crates/ferrite-pane/src/transcript.rs:2932-2933](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2932-L2933)
- [crates/ferrite-ui/src/motion.rs:64-70](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L64-L70)
- [apps/web/src/components/chat/MessagesTimeline.tsx:195](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L195)
- [apps/web/src/components/chat/MessagesTimeline.tsx:1449](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L1449)

## panel-tools-004

File discovery silently omits entries beyond fixed limits

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Large directories and repositories can contain files that the tree or fuzzy search never exposes, without a truncation notice.

Recommendation. Use incremental directory traversal and virtualized rows. If a limit remains, expose it and allow explicit loading or exact-path opening.

Acceptance check. Use a fixture with 1,100 files in one folder and a target after the 20,000th search entry. The final tree entry and target are reachable, or the UI explicitly provides continuation.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:735-750](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L735-L750)
- [crates/ferrite-cockpit/src/cockpit/tools/tree.rs:13-16](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/tree.rs#L13-L16)

## panel-tools-006

Restart restores the Files shell but loses its open files and tool preferences

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Operators lose their file working set, selected file, explorer visibility, editor preferences, Changes scope, and review state after restarting.

Recommendation. Persist canonical file surfaces, active file, and stable tool preferences. Define separate restoration rules for process resources and unsaved drafts.

Acceptance check. After restart, restore saved file tabs and active file, chosen tree visibility and wrapping, and the intended review scope. Unsaved drafts must be recovered or safely flushed, never silently lost.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:304-314](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L304-L314)
- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:138-149](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L138-L149)
- [crates/ferrite-cockpit/src/cockpit/tools/changes.rs:145-159](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/changes.rs#L145-L159)
- [apps/web/src/rightPanelStore.ts:812-844](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L812-L844)
- [apps/web/src/components/files/FilePreviewPanel.tsx:1348-1357](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1348-L1357)

## panel-tools-007

Markdown and prose files cannot be edited or navigated to a requested source line in Files

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

README files, instructions, and text documents open as rendered prose with no source-edit mode. Source links to their line numbers lose their target after redirecting into Files.

Recommendation. Separate presentation mode from editability. Preserve line intent and switch to source view for line links, with an explicit render/source toggle.

Acceptance check. Edit and save README.md and notes.txt. Follow README.md:80 from a conversation and verify source line 80 is visible and highlighted; switching back to rendered view preserves the file.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:192-196](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L192-L196)
- [crates/ferrite-pane/src/preview.rs:592-606](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L592-L606)
- [apps/web/src/components/files/FilePreviewPanel.tsx:1211-1235](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1211-L1235)

## panel-tools-008

Opening image or document files from the tree bypasses the available preview path

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

The Files tree tells users that an image is not text instead of showing it. PDFs, audio, video, and document previews also lack the richer in-panel handling available in T3.

Recommendation. Unify file opening around file-type dispatch. Reuse Ferrite's image viewer from Files and prioritize PDF, media, and table previews based on actual workflows.

Acceptance check. Open a PNG from both tree and transcript and reach equivalent image controls. Open a PDF and receive a preview or a specific supported fallback, not a generic text-decoding failure.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/files.rs:178-189](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/files.rs#L178-L189)
- [crates/ferrite-cockpit/src/cockpit/panel/preview.rs:1-6](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L1-L6)
- [crates/ferrite-pane/src/preview.rs:49-81](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L49-L81)
- [apps/web/src/components/files/FilePreviewPanel.tsx:193-225](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L193-L225)

## panel-tools-010

Disabling diff wrapping clips long lines instead of allowing horizontal scrolling

P2. defect. source-proven. Disposition `repair-or-quality`.

Long changed expressions become unreadable once wrap is disabled, even though that is the intended mode for comparing code alignment.

Recommendation. Use a horizontal scroll container for unwrapped diffs and preserve readable line-number and sign gutters.

Acceptance check. Put a changed token at column 250, disable wrapping, and scroll to inspect and select that token without resizing the whole window.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:334-357](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/hunks.rs#L334-L357)
- [crates/ferrite-cockpit/src/cockpit/tools/changes.rs:365](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/changes.rs#L365)
- [apps/web/src/components/DiffPanel.tsx:1515-1519](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L1515-L1519)

## panel-tools-011

Hide whitespace changes only removes blank changed lines

P2. defect. source-proven. Disposition `repair-or-quality`.

Indentation-only and trailing-space changes still dominate reviews when the operator explicitly asks to hide whitespace changes.

Recommendation. Compute the diff using an ignore-whitespace option and keep displayed statistics consistent with it.

Acceptance check. Use separate indentation-only, trailing-space-only, and substantive edits. With the option on, only substantive changes remain; turn it off to restore all changes.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/changes.rs:190-202](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/changes.rs#L190-L202)
- [apps/web/src/components/DiffPanel.tsx:373-384](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L373-L384)
- [apps/server/src/vcs/GitVcsDriverCore.ts:3748](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/vcs/GitVcsDriverCore.ts#L3748)

## panel-tools-012

Changes stays stale after workspace edits and reviewed marks survive changed contents

P2. defect. source-proven. Disposition `repair-or-quality`.

The review can show yesterday's loaded content while an agent keeps editing. A reviewed checkmark can remain on a file whose contents changed or whose comparison scope changed.

Recommendation. Refresh on a workspace revision signal and associate reviewed state with a diff/content identity. Preserve scroll and folds independently of review validity.

Acceptance check. Mark a file reviewed, modify it, and return to Changes. Updated hunks appear and the reviewed mark clears. A changed comparison base does not inherit unrelated reviewed marks.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/changes.rs:221-232](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/changes.rs#L221-L232)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:254-265](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L254-L265)
- [apps/web/src/components/DiffPanel.tsx:350-390](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L350-L390)

## panel-tools-013

Changes lacks full diff navigation and comparison controls present in T3

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Large code reviews require more scanning and external tooling. The panel cannot show old and new code side by side, reveal a file through a changes tree, or select a particular completed turn or commit.

Recommendation. Build on the existing review core while giving the panel a full diff presentation and explicit scope model. Treat turn history as a separate product decision if checkpoints are not available.

Acceptance check. Switch stacked/split views, jump to a changed file, read old/new line numbers and syntax, and compare a selected commit or completed turn without leaving the panel. If a capability is deferred, document the supported panel scope.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/changes.rs:5-17](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/changes.rs#L5-L17)
- [crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:21-26](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/hunks.rs#L21-L26)
- [apps/web/src/components/DiffPanel.tsx:329-364](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L329-L364)

## panel-tools-014

Stage and unstage failures are discarded

P2. defect. source-proven. Disposition `repair-or-quality`.

A Git lock, permission error, or repository failure makes the stage button appear to do nothing. The operator cannot tell whether the operation ran or what to fix.

Recommendation. Return a typed result from staging, display stderr on failure, prevent duplicate in-flight actions, and refresh only with a clear operation outcome.

Acceptance check. In a disposable repository, create a temporary index lock, click Stage, and see a precise error. Remove the lock and retry successfully. Do the equivalent for Unstage.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/changes.rs:180-187](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/changes.rs#L180-L187)

## panel-tools-016

Terminal surfaces cannot split into simultaneous shells

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Running a server and a command side by side requires switching tabs or using an external terminal.

Recommendation. Add split groups only if this everyday multitasking workflow is in scope; retain simple single-terminal tabs as the default.

Acceptance check. Run a server in one split and commands in the other, resize each pane, focus by keyboard, and close one pane without terminating its sibling.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:50-61](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/terminal.rs#L50-L61)
- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:29-42](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L29-L42)
- [apps/web/src/rightPanelStore.ts:42-47](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L42-L47)
- [apps/web/src/components/ThreadTerminalDrawer.tsx:1763-1786](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L1763-L1786)

## panel-tools-017

Terminal output has no URL or file-path opening flow

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users must copy a localhost URL or compiler file path manually instead of opening the browser or file directly from terminal output.

Recommendation. Parse URLs and file locations from terminal text or terminal hyperlink metadata and use the existing browser/file entry points.

Acceptance check. Print a localhost address, an HTTPS URL, and a relative path with a line number. Open each with the documented gesture and verify the correct destination and line.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-terminal/src/view.rs:570-612](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-terminal/src/view.rs#L570-L612)
- [crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:119-149](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/terminal.rs#L119-L149)
- [apps/web/src/components/ThreadTerminalDrawer.tsx:975-1024](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L975-L1024)

## panel-tools-018

Panel sizing cannot adapt to narrow layouts or recover a smaller width after wide tools

P2. interaction. inferred. Disposition `repair-or-quality`.

The panel can consume disproportionate space on a small window. Opening Files or Changes permanently raises the saved width until the operator resizes it, even after returning to Overview.

Recommendation. Define narrow-window behavior and preserve a user-chosen compact width separately from temporary tool minimums. Consider a sheet or explicit panel-only mode.

Acceptance check. At 800, 1,000, and 1,400 pixel window widths, open each tool and return to Overview. All controls remain reachable and chat remains usable; temporary tool expansion does not silently replace the operator's compact width.

[Full investigation and reproduction conditions](reports/panel-tools.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel.rs:149-177](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L149-L177)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:368-391](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L368-L391)
- [crates/ferrite-ui/src/theme.rs:989](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/theme.rs#L989)
- [apps/web/src/rightPanelLayout.ts:1-3](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelLayout.ts#L1-L3)
- [apps/web/src/components/chat/ChatRightPanel.tsx:14-49](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatRightPanel.tsx#L14-L49)
- [apps/web/src/components/RightPanelSheet.tsx:20-25](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelSheet.tsx#L20-L25)

Review note. The sticky-width branch is runtime-reproduced: Overview 320px becomes 864px after Files/Changes, auto-hides navigation and remains wide through close/reopen and restart at a 1440px window. Narrow-window/manual-drag branches remain unverified.

Runtime evidence:

- [panel-runtime-correction.md](reports/panel-runtime-correction.md)
- [overview-width-contract.json](evidence/panel-correction/overview-width-contract.json)
- [03-overview-reopened.png](evidence/panel-correction/03-overview-reopened.png)
- [05-overview-after-files.png](evidence/panel-correction/05-overview-after-files.png)

## pr-navigation-02

The main PR affordances bypass the native PR view

P2. interaction. source-proven. Disposition `repair-or-quality`.

The sidebar PR badge and footer PR button open a generic Browser tab, while the Overview PR row and panel menu open the native Summary/Guide/Timeline/Code view. Users can repeatedly choose the obvious PR controls without discovering the built-in review interface.

Recommendation. Make the badge and current-PR action open the native PR view by default. Preserve separately named browser and external-browser actions. Do not relabel the existing Open PR in browser command as broken merely because it does what its name says.

Acceptance check. For the focused thread, the badge and current-PR action show the same native PR tab as Overview and the + menu. Repeated opens leave one native tab. Explicit browser actions still open the browser.

[Full investigation and reproduction conditions](reports/pr-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:314-339](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L314-L339)
- [crates/ferrite-cockpit/src/cockpit.rs:1826-1831](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L1826-L1831)
- [crates/ferrite-cockpit/src/cockpit.rs:12806-12819](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12806-L12819)
- [crates/ferrite-cockpit/src/cockpit/panel/changes.rs:379-397](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/changes.rs#L379-L397)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:159-171](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L159-L171)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:452-469](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L452-L469)
- [apps/web/src/components/sidebar/SidebarThreadRow.tsx:962-969](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969)
- [apps/web/src/lib/openPullRequestLink.ts:74-137](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/openPullRequestLink.ts#L74-L137)

Review note. Focused sidebar badge opened Browser; the Overview PR row opened the real native Summary and Timeline through production reviewr reads. Other PR entry points remain source-only.

Runtime evidence:

- [pr-entry-runtime-correction.md](reports/pr-entry-runtime-correction.md)
- [02-focused-thread-pr-click.png](evidence/pr-entry-correction/02-focused-thread-pr-click.png)
- [07-native-summary.png](evidence/pr-entry-correction/07-native-summary.png)
- [08-native-timeline.png](evidence/pr-entry-correction/08-native-timeline.png)

## pr-navigation-03

There is no standalone PR inbox or project-scoped PR browsing destination

P2. missing-capability. source-proven. Disposition `repair-or-quality`.

Ferrite cannot use the sidebar footer to browse PRs independently of a focused branch. With no known PR on the focused thread, that control does nothing even when the project has other PRs. Reviewers cannot inspect authored, reviewing, or other project PRs through an app-wide list.

Recommendation. Add a distinct PR inbox destination with project scope and explicit selection, usable without a focused thread. Keep the current branch shortcut separate. Scope and filter controls should represent actual supported backend queries.

Acceptance check. From an empty board and a thread without a PR, open Pull requests, select a project, inspect a PR from a different branch, change list scope, and return to the previously selected PR without changing checkout.

[Full investigation and reproduction conditions](reports/pr-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:324-339](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L324-L339)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:136-144](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L136-L144)
- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:25-45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L25-L45)
- [crates/ferrite-cockpit/src/cockpit/starts.rs:790-825](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/starts.rs#L790-L825)
- [crates/ferrite-cockpit/src/cockpit/starts.rs:1055-1074](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/starts.rs#L1055-L1074)
- [apps/web/src/components/sidebar/SidebarChrome.tsx:234-239](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarChrome.tsx#L234-L239)
- [apps/web/src/routes/_chat.pull-requests.tsx:30-78](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/routes/_chat.pull-requests.tsx#L30-L78)
- [apps/web/src/components/pullRequest/usePullRequestListView.ts:89-123](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestListView.ts#L89-L123)
- [apps/web/src/components/pullRequest/PullRequestsPage.tsx:1123-1132](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1123-L1132)

Review note. Surface-composition reconciliation corrects prior optional-scope framing: the user explicitly requires an accessible PRs view. A standalone PR browsing destination is required audit repair scope, usable from an empty board or a thread without a current-branch PR. Native forge mutation features remain separate choices. Evidence remains source-proven; no new runtime pass is inferred.

## pr-navigation-04

The native PR target is the current branch, so links and multiple PRs cannot become independent native tabs

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A native tab stores only a number under a thread, and its data loader resolves the current checkout rather than an explicit repository and PR. Ferrite cannot natively open an arbitrary linked PR beside the current PR, retain an older PR after the branch points to a newer one, or navigate a PR comment anchor into the native view.

Recommendation. Introduce an explicit selected PR identity containing repository, host, number, and environment or project scope. Read by that target. Retain branch PR discovery as an entry point, then allow independent selected PR tabs and anchor reveals.

Acceptance check. Open PR 48 and PR 49 from links in the same thread, then switch its branch. Each tab still names and loads its own PR. Equal numbers from different repositories remain distinct. A linked comment or diff anchor reveals the intended content.

[Full investigation and reproduction conditions](reports/pr-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:44-45](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L44-L45)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:274-305](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L274-L305)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:319-352](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L319-L352)
- [crates/ferrite-cockpit/src/cockpit.rs:3508-3521](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L3508-L3521)
- [crates/ferrite-pane/src/preview.rs:322-331](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L322-L331)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:342-355](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L342-L355)
- [apps/web/src/rightPanelStore.ts:66-80](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L66-L80)
- [apps/web/src/rightPanelStore.ts:267-298](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L267-L298)
- [apps/web/src/lib/openPullRequestLink.ts:83-134](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/openPullRequestLink.ts#L83-L134)
- [apps/web/src/components/ChatMarkdown.tsx:1257-1261](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1257-L1261)
- [apps/web/src/components/sidebar/SidebarThreadRow.tsx:1074-1091](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1074-L1091)

## pr-navigation-05

A known PR without a URL still presents clickable controls that silently do nothing

P2. defect. source-proven. Disposition `repair-or-quality`.

The accepted PR data shape permits a number and state without a URL. The sidebar still underlines the number and the footer appears enabled, but open_pr returns without feedback. The native PR entry could still work because it needs a number and checkout rather than the missing URL.

Recommendation. Use the native PR destination when number and checkout are available. For URL-only actions, apply the same URL availability gate already used by the context menu and palette, with a clear unavailable reason.

Acceptance check. Render a PR reading with number 415 and url None. Its primary PR control opens native detail or clearly states why unavailable; it never appears as an enabled dead link. Normal production gh usually supplies a URL, so also verify the real failing data before attributing the user's incident to this case.

[Full investigation and reproduction conditions](reports/pr-navigation.md).

Evidence:

- [crates/ferrite-core/src/workspace/mod.rs:539-546](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/workspace/mod.rs#L539-L546)
- [crates/ferrite-core/src/workspace/mod.rs:768-783](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/workspace/mod.rs#L768-L783)
- [crates/ferrite-cockpit/src/nav/card.rs:787-809](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/nav/card.rs#L787-L809)
- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:324-339](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L324-L339)
- [crates/ferrite-cockpit/src/cockpit.rs:1827-1831](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L1827-L1831)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:274-276](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L274-L276)
- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:460-462](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L460-L462)

## pr-navigation-06

The reported sidebar click failure lacks end-to-end verification at the actual entry points

P2. verification-gap. unverified. Disposition `verification-gap`.

Existing PR tests establish the internal panel path but do not establish that the reported sidebar click works in the installed app. The source explains nonfocused and missing-URL failures, but the user may also hit a separate native browser or pointer problem when clicking a focused thread.

Recommendation. Add an event-level regression test for the real sidebar badge, footer, and + menu. Have the runtime owner exercise the installed app with known URLs on focused and nonfocused threads. Assert destination visibility and selection, not just tab allocation or handler invocation.

Acceptance check. Record installed build identity, clicked thread, PR URL availability, focused thread, panel active tab before and after, and visible destination. Cover plain and modifier clicks and panel-hidden state. Verify a test fails if the badge handler stops activating its intended destination.

[Full investigation and reproduction conditions](reports/pr-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/tests/pr.rs:92-134](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/pr.rs#L92-L134)
- [crates/ferrite-cockpit/src/cockpit/tests/pr.rs:226-247](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/pr.rs#L226-L247)
- [crates/ferrite-cockpit/src/cockpit/tests/nav_shelves.rs:225-239](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/nav_shelves.rs#L225-L239)
- [crates/ferrite-cockpit/src/cockpit/tests/nav_shelves.rs:384-418](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tests/nav_shelves.rs#L384-L418)
- [apps/web/src/components/sidebar/SidebarThreadRow.tsx:962-969](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L962-L969)

## pr-operations-002

Reload discards the last good PR and replaces it with a transient error

P2. defect. source-proven. Disposition `repair-or-quality`.

A temporary network or authentication failure removes previously readable review context, including access to the Code tab body.

Recommendation. Store last-success, pending and error separately. Preserve content on retriable failures and clear it only when authoritative identity or absence changes.

Acceptance check. Load a PR, simulate a failed refresh, and verify that title, discussion and diff remain visible with an actionable stale/error indicator. Retry succeeds without closing the tab.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:32-36](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L32-L36)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:293-305](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L293-L305)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:461-469](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L461-L469)
- [crates/ferrite-core/src/protocol/review.rs:689-708](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L689-L708)
- [apps/web/src/components/pullRequest/usePullRequestDetail.ts:56-74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L56-L74)
- [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:624-672](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L624-L672)

## pr-operations-003

Authentication and retry guidance do not form a working recovery flow

P2. defect. source-proven. Disposition `repair-or-quality`.

Users see no PR or a dead-end instruction instead of a recoverable authentication error. Missing CLI, expired credentials and a branch with no PR look identical during discovery.

Recommendation. Propagate typed discovery failures to the PR opener and panel. Add a retry button bound to detail reload and make the displayed shortcut match the real binding. Keep host-specific login commands.

Acceptance check. With a mocked missing executable and then an expired login, show distinct remedies. After restoring authentication, clicking Retry must load the PR. The documented keyboard action must do the same.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-core/src/workspace/mod.rs:724-749](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/workspace/mod.rs#L724-L749)
- [crates/ferrite-core/src/protocol/review.rs:732-747](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L732-L747)
- [crates/ferrite-reviewr/src/forge.rs:44-75](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L44-L75)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:294-305](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L294-L305)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:867-878](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L867-L878)
- [crates/ferrite-cockpit/src/cockpit.rs:10483-10486](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L10483-L10486)
- [crates/ferrite-ui/src/keymap.rs:631](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/keymap.rs#L631)
- [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:64-69](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L64-L69)
- [packages/source-control-github/src/server/GitHubRequestPolicy.ts:36-64](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubRequestPolicy.ts#L36-L64)
- [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:624-676](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L624-L676)

## pr-operations-004

Rewrite guide displays the oldest matching guide

P2. defect. source-proven. Disposition `repair-or-quality`.

A rewritten guide can appear not to have changed, and reviewers can read superseded instructions.

Recommendation. Select the newest by timestamp or honor the canonical newest-first contract. Add a fixture with multiple guides in real backend order.

Acceptance check. With older and newer guide comments from a human account, Guide displays the newer one. Rewriting and refreshing must replace it with the latest text.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-reviewr/src/forge.rs:1025-1028](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L1025-L1028)
- [crates/ferrite-core/src/protocol/review.rs:652-678](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L652-L678)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:224-235](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L224-L235)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:749-756](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L749-L756)
- [apps/web/src/components/pullRequest/PullRequestGuideTab.tsx:89-99](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestGuideTab.tsx#L89-L99)
- [packages/contracts/src/pullRequestReviewGuide.ts:45-69](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/pullRequestReviewGuide.ts#L45-L69)

## pr-operations-006

Timeline drops review replies, resolution state and quoted code

P2. defect. source-proven. Disposition `repair-or-quality`.

Reviewers cannot see answers or distinguish open findings from already resolved or outdated discussions. A finding loses the code context needed to evaluate it.

Recommendation. Render complete review threads and their state. Reuse the canonical thread model for Summary, Timeline and inline code comments.

Acceptance check. A fixture with a resolved, outdated thread and two replies exposes all four facts. The user can open the associated code location and distinguish it from an unresolved thread.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-core/src/protocol/review.rs:575-610](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L575-L610)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:789-840](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L789-L840)
- [crates/ferrite-cockpit/src/review.rs:1898-1953](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/review.rs#L1898-L1953)
- [apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx:462-543](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx#L462-L543)
- [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:620-631](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L620-L631)

## pr-operations-007

Capped PR data is presented as a complete conversation and check result

P2. defect. source-proven. Disposition `repair-or-quality`.

A reviewer can mistake a partial list for all findings or all checks. Missing failure data can make the summary appear more reassuring than the complete PR.

Recommendation. Show explicit partial-data notices separately for checks and discussion. Page remaining data where supported and withhold a complete-pass claim until coverage is known.

Acceptance check. Inject checks_truncated and comments_truncated independently. Each produces its own notice and route to more data. A truncated success-only subset never claims all checks pass.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-reviewr/src/forge.rs:853-865](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L853-L865)
- [crates/ferrite-reviewr/src/forge.rs:964-974](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L964-L974)
- [crates/ferrite-core/src/protocol/review.rs:496-517](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L496-L517)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:246-263](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L246-L263)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:689-745](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L689-L745)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:789-840](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L789-L840)
- [crates/ferrite-cockpit/src/review.rs:1952-1953](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/review.rs#L1952-L1953)
- [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:1219-1221](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L1219-L1221)
- [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:620-623](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L620-L623)

## pr-operations-008

Checks stop at names and icons without a route to failure details

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A failed job requires manually finding the run on the forge. Users cannot inspect the result from the displayed check or distinguish some important status categories.

Recommendation. Carry check URLs and useful status distinctions through the model. Add a Details action with visible open failure handling.

Acceptance check. A failed job opens its own run page from the PR panel. Cancelled and approval-required runs have distinct labels and appropriate next actions.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-reviewr/src/forge.rs:859-860](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L859-L860)
- [crates/ferrite-reviewr/src/forge.rs:1047-1064](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L1047-L1064)
- [crates/ferrite-core/src/protocol/review.rs:557-572](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L557-L572)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:702-716](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L702-L716)
- [apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx:55-109](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx#L55-L109)
- [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:1263-1279](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L1263-L1279)

## pr-operations-009

Descriptions, comments and guides render as raw text

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

PR templates, code fences, tables, checklists, image evidence and linked documents lose their intended presentation and interaction.

Recommendation. Use a shared Markdown component with forge-relative links, image handling, code copy and safe task-list interaction.

Acceptance check. An example PR with a linked issue, fenced code, table, checklist and image renders each correctly. Links open their actual destinations and long code remains usable.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:727-733](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L727-L733)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:752-756](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L752-L756)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:832-837](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L832-L837)
- [apps/web/src/components/pullRequest/PullRequestMarkdown.tsx:17-28](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestMarkdown.tsx#L17-L28)
- [apps/web/src/components/pullRequest/PullRequestMarkdown.tsx:30-113](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestMarkdown.tsx#L30-L113)
- [apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx:647-663](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx#L647-L663)

## pr-operations-010

No native comment, reply, review verdict or resolution workflow

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users must leave the PR pane or ask an agent to conduct the review. They cannot prepare a batch of inline comments and submit an approval or request changes in the UI.

Recommendation. Add typed, permission-aware native review operations or explicitly label the limited workflow and provide reliable forge handoff. Keep failed review drafts.

Acceptance check. Compose an inline comment and approval, fail the submit once, retry without losing text, then see the posted review. Reply and resolve a thread and verify the new state after refresh.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:99-116](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L99-L116)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:429-449](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L429-L449)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:789-840](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L789-L840)
- [crates/ferrite-core/src/protocol/review.rs:575-610](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L575-L610)
- [apps/web/src/components/pullRequest/PullRequestReviewForm.tsx:27-48](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestReviewForm.tsx#L27-L48)
- [apps/web/src/components/pullRequest/PullRequestReviewForm.tsx:100-148](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestReviewForm.tsx#L100-L148)
- [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:755-805](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L755-L805)

## pr-operations-011

PR metadata cannot be edited or managed in the native panel

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users cannot correct a title or description, request reviewers, or apply labels while reviewing in Ferrite.

Recommendation. Extend the domain model and add focused editors with explicit saving/error states and permission checks.

Acceptance check. Edit title/body and change labels/review requests without leaving the panel. A rejected save retains the draft and shows the reason.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-core/src/protocol/review.rs:478-505](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L478-L505)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:99-116](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L99-L116)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:717-745](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L717-L745)
- [apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx:647-663](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx#L647-L663)
- [apps/web/src/components/pullRequest/PullRequestReviewerPicker.tsx:66-107](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestReviewerPicker.tsx#L66-L107)
- [apps/web/src/components/pullRequest/PullRequestLabelPicker.tsx:35-51](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestLabelPicker.tsx#L35-L51)
- [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:740-753](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L740-L753)

## pr-operations-012

Agent-backed PR operations have no panel-level pending, success or failure lifecycle

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Clicking Merge, Ready, Write guide or Create PR can appear to do nothing while work queues behind another turn. Repeated clicks can enqueue repeated requests.

Recommendation. Keep agent execution if desired, but model an operation with a target, state and result. Show that state beside the initiating control and deduplicate repeated requests.

Acceptance check. Click an action while a turn is busy. Show Queued and the target PR, block duplicate enqueue, then transition through Running to success/failure and refresh the affected data.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:9-15](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L9-L15)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:429-449](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L429-L449)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:593-602](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L593-L602)
- [crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:358-380](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/transcript_glue.rs#L358-L380)
- [crates/ferrite-cockpit/src/cockpit/panel/changes.rs:119-134](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/changes.rs#L119-L134)
- [apps/web/src/components/pullRequest/usePullRequestActions.ts:127-186](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L127-L186)
- [apps/server/src/git/GitManager.ts:2457-2497](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/git/GitManager.ts#L2457-L2497)

## pr-operations-013

Lifecycle actions lack inverse operations and branch maintenance

P2. missing-capability. source-proven. Disposition `capability-candidate`.

A user can enable auto-merge or close a PR but cannot undo those choices from the same UI. Updating a stale branch and approving blocked workflows also require leaving the panel.

Recommendation. Model the current settings and complete reversible lifecycle actions. Gate them by actual host capability and permission.

Acceptance check. Enable then disable auto-merge, close then reopen a PR, toggle maintainer edits both ways, and update an eligible branch. Each result appears after refresh.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:99-116](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L99-L116)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:147-155](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L147-L155)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:189-197](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L189-L197)
- [crates/ferrite-core/src/protocol/review.rs:478-505](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L478-L505)
- [apps/web/src/components/pullRequest/usePullRequestActions.ts:79-104](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L79-L104)
- [apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx:288-311](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L288-L311)
- [apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx:341-344](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L341-L344)
- [apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx:434-440](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestActionsMenu.tsx#L434-L440)

## pr-operations-014

PR action prompts lose canonical host and repository identity

P2. partial-capability. inferred. Disposition `repair-or-quality`.

For forks, multiple remotes, enterprise hosts or non-GitHub forges, the agent must reconstruct the target and may fail or act in the wrong repository. No wrong-target mutation was performed in this audit.

Recommendation. Carry an explicit target identity through every operation. For agent handoffs, include the URL and provider-specific command plus a target-verification step. Resolve branch deletion from the PR head repository.

Acceptance check. With origin=fork and upstream=base, generated operations target the named PR and correct remote branch. GitLab/Azure reads generate matching provider operations or clearly unavailable controls.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:159-205](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L159-L205)
- [crates/ferrite-core/src/protocol/review.rs:612-626](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L612-L626)
- [crates/ferrite-reviewr/src/forge.rs:522-540](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L522-L540)
- [crates/ferrite-reviewr/src/ferrite.rs:649-659](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/ferrite.rs#L649-L659)
- [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:722-780](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L722-L780)
- [packages/source-control-core/src/server/PullRequestProvider.ts:495-515](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-core/src/server/PullRequestProvider.ts#L495-L515)

## pr-operations-015

Concurrent PR reads can land out of order and retire the current tab

P2. defect. inferred. Disposition `repair-or-quality`.

A slow old response can overwrite newer details, or remove the tab for the PR now on the branch. The source permits this race; timing was not reproduced.

Recommendation. Capture canonical target and request generation, deduplicate pending reads, and reject stale completions before they modify cache or surfaces.

Acceptance check. Delay read A, start read B for a different current PR, complete B then A. B stays visible and its tab remains. Repeated open of one pending PR creates one request.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel.rs:81](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L81)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:283-309](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L283-L309)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:338-351](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L338-L351)
- [crates/ferrite-cockpit/src/cockpit/panel.rs:254-266](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L254-L266)
- [apps/web/src/components/pullRequest/usePullRequestDetail.ts:49-68](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestDetail.ts#L49-L68)
- [apps/web/src/components/pullRequest/usePullRequestActions.ts:144-160](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L144-L160)

## pr-operations-016

PR read workers have no effective cancellation or deadline

P2. defect. source-proven. Disposition `repair-or-quality`.

A hung forge process can leave Reading visible indefinitely and retain a worker after the pane is closed. Repeated forced opens can accumulate stuck reads.

Recommendation. Thread cancellation and a deadline through the one-shot adapter; cancel on supersession and release the process tree. Share equivalent safeguards with badge discovery.

Acceptance check. A fake gh that never exits reaches a visible timeout. Closing or superseding the request terminates its process tree, and repeated retries do not leave worker processes behind.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:294-309](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L294-L309)
- [crates/ferrite-reviewr/src/ferrite.rs:652-659](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/ferrite.rs#L652-L659)
- [crates/ferrite-reviewr/src/forge.rs:338-352](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L338-L352)
- [crates/ferrite-reviewr/src/forge.rs:484-485](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L484-L485)
- [crates/ferrite-core/src/server.rs:1118-1129](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/server.rs#L1118-L1129)
- [crates/ferrite-core/src/cockpit.rs:1251-1259](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L1251-L1259)
- [packages/source-control-github/src/server/GitHubRequestPolicy.ts:21-24](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubRequestPolicy.ts#L21-L24)
- [packages/source-control-github/src/server/GitHubRequestPolicy.ts:427-434](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubRequestPolicy.ts#L427-L434)

## pr-operations-017

Review verdicts and empty-body approvals are absent from the snapshot

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Reviewers cannot tell who approved, requested changes, or had a review dismissed. An approval with no written body disappears entirely.

Recommendation. Preserve structured review outcomes, including empty-body verdicts and whether the review applies to the latest head.

Acceptance check. An approval with an empty body appears with author and verdict. A later push marks an old review appropriately. Changes requested and dismissed reviews remain distinguishable.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-reviewr/src/forge.rs:861](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L861)
- [crates/ferrite-reviewr/src/forge.rs:1072-1078](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L1072-L1078)
- [crates/ferrite-core/src/protocol/review.rs:575-603](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L575-L603)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:795-804](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L795-L804)
- [apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx:522-542](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx#L522-L542)
- [apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx:723-747](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestSummaryTab.tsx#L723-L747)
- [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:600-609](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L600-L609)

## pr-operations-018

PR panel omits the checkout-versus-host sync warning

P2. defect. source-proven. Disposition `repair-or-quality`.

Users may confuse local changes shown in Code with what reviewers can actually see on the host, because the PR view drops the unpushed/behind warning.

Recommendation. Restore the sync notice and explicitly identify which revision Code displays. Prefer the hosted PR revisions for hosted review operations.

Acceptance check. With two unpushed commits, the PR panel says so and identifies the displayed code revision. With a behind checkout, it does not silently present the local diff as the current hosted PR.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-core/src/protocol/review.rs:490-491](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L490-L491)
- [crates/ferrite-core/src/protocol/review.rs:540-554](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L540-L554)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:466](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L466)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:689-701](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L689-L701)
- [crates/ferrite-cockpit/src/cockpit/tools/changes.rs:71-84](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/tools/changes.rs#L71-L84)
- [crates/ferrite-cockpit/src/review.rs:2519-2541](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/review.rs#L2519-L2541)
- [apps/web/src/components/pullRequest/PullRequestCodeTab.tsx:435-464](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L435-L464)
- [apps/web/src/components/pullRequest/PullRequestCodeTab.tsx:649-652](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestCodeTab.tsx#L649-L652)

## pr-operations-019

Guide is a comment convention without revision-aware review workflow

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A guide can remain apparently current after a push, and its file references cannot act as a structured review tour. Guide generation has no local run or failure state.

Recommendation. Decide whether a comment guide is an intentional reduced feature. At minimum record head identity, generation state and stale status. Full parity needs ordered stops with linked excerpts.

Acceptance check. After generating a guide, push a new commit. The guide is visibly stale. A failed rewrite retains the old guide and offers retry; successful stops open the corresponding file and lines.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:176-179](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L176-L179)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:224-235](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L224-L235)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:749-784](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L749-L784)
- [crates/ferrite-core/src/protocol/review.rs:478-505](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L478-L505)
- [packages/contracts/src/pullRequestReviewGuide.ts:12-69](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/pullRequestReviewGuide.ts#L12-L69)
- [apps/web/src/components/pullRequest/PullRequestGuideTab.tsx:89-145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestGuideTab.tsx#L89-L145)

## pr-operations-021

Merge controls ignore permissions, allowed methods and readiness

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

The panel offers actions the user cannot perform and always presents Merge on an open PR, even with conflicts or blocked checks. The user cannot choose a supported merge method.

Recommendation. Fetch viewer permissions, available merge methods and readiness. Disable or relabel unavailable actions, let users select a method, and bind confirmation to the intended target/revision.

Acceptance check. For a read-only viewer there is no enabled Merge. A conflicting PR displays the blocker. A squash-only repository offers the correct method and the operation reports its actual result.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:147-155](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L147-L155)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:185-191](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L185-L191)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:568-601](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L568-L601)
- [crates/ferrite-core/src/protocol/review.rs:478-505](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L478-L505)
- [apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx:1267-1291](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L1267-L1291)
- [apps/web/src/components/pullRequest/usePullRequestActions.ts:49-76](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/usePullRequestActions.ts#L49-L76)
- [packages/source-control-github/src/server/GitHubPullRequestProvider.ts:76-105](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/source-control-github/src/server/GitHubPullRequestProvider.ts#L76-L105)

## projects-navigation-002

Reopen failures produce a log entry without a visible error

P2. defect. source-proven. Disposition `repair-or-quality`.

A parked thread can appear unresponsive when its stored history cannot be loaded. The search also clears before the failure, removing the user's recovery context.

Recommendation. Return a visible per-thread recovery message and retain the row and query on failure. Let Undo report failed restores instead of consuming its notice silently.

Acceptance check. With a controlled unreadable or missing thread log, click the parked row, invoke Reopen last, and undo Park. Each path identifies the failed thread, shows the failure, and leaves a retryable state.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_list.rs:418-426](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_list.rs#L418-L426)
- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:261-264](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L261-L264)
- [crates/ferrite-cockpit/src/cockpit.rs:9063-9072](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L9063-L9072)
- [crates/ferrite-core/src/cockpit.rs:3099-3106](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L3099-L3106)
- [crates/ferrite-core/src/engine.rs:2360-2372](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L2360-L2372)
- [apps/web/src/components/settings/ArchivedThreadsPanel.tsx:138-168](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ArchivedThreadsPanel.tsx#L138-L168)
- [apps/web/src/hooks/useThreadActions.ts:363-381](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActions.ts#L363-L381)

## projects-navigation-003

Undo Park does not restore group membership or a dissolved group

P2. defect. source-proven. Disposition `repair-or-quality`.

Undo restores a transcript but cannot restore the grouped workspace that Park changed. Users can lose their arrangement after what appears to be a reversible action.

Recommendation. Capture the group and placement before parking, then restore surviving membership and layout on Undo. Report conflicts when concurrent edits prevent exact restoration.

Acceptance check. Create a two-thread group, park one member, then Undo. Both threads must return to the original group and arrangement. Repeat with multi-selection and a larger group.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-core/src/cockpit.rs:3308-3332](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L3308-L3332)
- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:20-24](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L20-L24)
- [crates/ferrite-cockpit/src/cockpit.rs:12830-12841](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12830-L12841)
- [apps/web/src/hooks/useThreadActions.ts:814-835](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActions.ts#L814-L835)

## projects-navigation-004

Bulk Park discards individual failures and clears the selection

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A mixed success/failure batch reports only Parked. The operator cannot tell which requested parks failed without inspecting the remaining list.

Recommendation. Collect a result for every requested thread. Show the successful and failed counts, retain failures in the selection, and offer Undo only for the successful subset.

Acceptance check. Force one of three selected threads to refuse Park. The UI says two parked and names the failure. The failing thread stays selected. Undo restores only the two successful parks.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:12832-12842](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12832-L12842)
- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:692-696](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L692-L696)
- [crates/ferrite-cockpit/src/cockpit/nav_pick.rs:125-133](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_pick.rs#L125-L133)
- [crates/ferrite-core/src/engine.rs:2308-2311](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L2308-L2311)
- [apps/web/src/components/Sidebar.tsx:2433-2462](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2433-L2462)

## projects-navigation-005

Sidebar search matches titles only

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A thread cannot be found from its branch, PR number, or remembered message text through the sidebar search, even though the result row itself displays a PR number.

Recommendation. Extend a shared thread-search model to branch and PR metadata, then add cancellable content search with snippets and navigation to the matched message. Keep the current project scope explicit.

Acceptance check. Search a unique branch, a PR number, and text found only inside a parked transcript. Each query returns the correct thread. A content result opens at the matching message.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:71-83](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L71-L83)
- [crates/ferrite-cockpit/src/nav/chrome.rs:193-201](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/nav/chrome.rs#L193-L201)
- [apps/web/src/components/Sidebar.logic.ts:1122-1158](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.logic.ts#L1122-L1158)
- [apps/web/src/components/Sidebar.tsx:1511-1520](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1511-L1520)

## projects-navigation-006

Keyboard search navigation does not reveal the highlighted result

P2. defect. source-proven. Disposition `repair-or-quality`.

Arrow-key navigation can move the highlighted search result beyond the viewport while the keyboard remains in the search field. Enter then opens an unseen row.

Recommendation. Give search results the same explicit reveal behavior as ordinary nav rows. Reveal on query reset and every highlight change without moving keyboard focus out of the field.

Acceptance check. Populate enough matching threads to exceed the viewport. Press Down to the final result and Up to the first. Each highlighted row must remain visible throughout.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:203-244](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L203-L244)
- [crates/ferrite-cockpit/src/nav/chrome.rs:160-213](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/nav/chrome.rs#L160-L213)
- [crates/ferrite-cockpit/src/cockpit/nav_keys.rs:121-138](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_keys.rs#L121-L138)
- [apps/web/src/components/sidebar/SidebarThreadRow.tsx:1626-1630](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1626-L1630)

Review note. Opening a broad search after sidebar scrolling retained the old scroll offset with cursor reset to result 0. Visible rows began at 09, but Enter opened the invisible selected result 02. A separate visible-result Down/Enter path worked. Live reordering remains untested.

Runtime evidence:

- [navigation-runtime-correction.md](reports/navigation-runtime-correction.md)
- [14-search-offscreen-zero.png](evidence/navigation-correction/14-search-offscreen-zero.png)
- [15-enter-offscreen-zero-opens-02.png](evidence/navigation-correction/15-enter-offscreen-zero-opens-02.png)

## projects-navigation-007

Live search results can change the Enter target or make Enter a no-op

P2. defect. source-proven. Disposition `repair-or-quality`.

A background status change can reorder the results under the user's highlight. A result disappearing can leave a visibly selected row that Enter will not open.

Recommendation. Store the selected ThreadId and resolve its current index. Use one resolved selection for drawing and activation. Pick a documented fallback when that thread disappears.

Acceptance check. Highlight a search result, then cause another matching thread to move between Active and Working. Enter must still open the highlighted identity. Remove the selected final result and confirm the visible fallback opens.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:57-59](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L57-L59)
- [crates/ferrite-cockpit/src/cockpit.rs:8841-8867](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L8841-L8867)
- [apps/web/src/components/Sidebar.tsx:1429-1443](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1429-L1443)

## projects-navigation-008

Project scope has no search, multi-project selection, or persistence

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Large project lists require manual scanning. Users cannot watch a chosen subset of projects together, and the chosen project scope is lost on a new view.

Recommendation. Use a searchable scope picker with explicit All and selected-project states. Persist the selection if the product intends it to survive restart. Multi-project scope should remain optional.

Acceptance check. With 30 projects, type a project name into the filter, select two projects, and see only their threads. Reopen the application and confirm the selected scope is restored.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:280-309](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L280-L309)
- [crates/ferrite-cockpit/src/cockpit.rs:429](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L429)
- [apps/web/src/components/Sidebar.tsx:1148-1173](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1148-L1173)

## projects-navigation-009

Opening a thread outside the project filter leaves its sidebar selection hidden

P2. interaction. source-proven. Disposition `repair-or-quality`.

The main view can show a thread with no selected row in the nav. This weakens the orientation promise that selection follows the shown thread.

Recommendation. On explicit cross-project navigation, reveal the destination by widening the filter or drawing a clearly labeled current-thread exception. Preserve the user's scope on background updates.

Acceptance check. Filter to project A, then open a thread in B through history or the palette. The destination has a visible selected nav row or an explicit outside-filter indicator. Background updates alone must not change the scope.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:8809-8826](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L8809-L8826)
- [crates/ferrite-cockpit/src/cockpit/history.rs:207-214](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/history.rs#L207-L214)
- [crates/ferrite-cockpit/src/cockpit/nav_list.rs:295-296](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_list.rs#L295-L296)
- [apps/web/src/components/Sidebar.tsx:1148-1174](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1148-L1174)

## projects-navigation-010

Pinned rows cannot be reordered directly

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users cannot arrange persistent pinned priorities by dragging. Unpinning and repinning moves only to the end, which is a cumbersome ordering substitute.

Recommendation. Add an explicit reorder destination for pinned rows and persist the resulting order. Preserve the existing drag-to-board behavior by distinguishing its destination.

Acceptance check. Pin three threads, drag the final one to the first slot, restart, and see the same order. Dragging into the board must still use the established grouping behavior.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:7240-7253](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L7240-L7253)
- [crates/ferrite-cockpit/src/cockpit/nav_list.rs:275-277](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_list.rs#L275-L277)
- [apps/web/src/components/Sidebar.logic.ts:465-484](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.logic.ts#L465-L484)

## projects-navigation-011

Multi-selection applies only to Park and is absent from slim rows

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

The UI suggests a multi-thread selection model but Snooze, Mark unread, Unpin and Delete still act on one context-menu thread. Parked and Snoozed rows cannot join that selection with the same gesture.

Recommendation. Make the supported batch operations explicit and label them with their target count. Apply the selection model consistently to applicable shelves. Do not imply a batch for actions that remain singular.

Acceptance check. Select three rows and invoke each supported batch action. Its label names all three targets and its outcome accounts for each. Cmd-click on a Snoozed or Parked row follows the same documented selection behavior.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/nav_pick.rs:45-88](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_pick.rs#L45-L88)
- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:421-434](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L421-L434)
- [crates/ferrite-cockpit/src/cockpit/nav_list.rs:327-335](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_list.rs#L327-L335)
- [apps/web/src/components/Sidebar.tsx:2352-2402](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2352-L2402)

## projects-navigation-012

Project editing and removal lack a contextual navigation entry point

P2. interaction. source-proven. Disposition `repair-or-quality`.

After the project tree retired, users must discover the command palette to rename a project, manage its directories, or remove it. These capabilities exist but the normal project filter does not lead to them.

Recommendation. Add Edit project to the selected project's filter entry or to the thread context menu. Reuse the existing editor and its validation.

Acceptance check. Starting with a visible thread, reach its project editor with the pointer, rename the project, and manage its directories without opening the command palette.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [docs/adr/0040-the-nav-is-the-inbox.md:24-28](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/adr/0040-the-nav-is-the-inbox.md#L24-L28)
- [crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:280-309](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs#L280-L309)
- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:401-495](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L401-L495)
- [crates/ferrite-cockpit/src/cockpit/palette.rs:883-890](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/palette.rs#L883-L890)
- [crates/ferrite-cockpit/src/cockpit.rs:4883-4909](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4883-L4909)
- [apps/web/src/components/threadActionMenu.logic.ts:241](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/threadActionMenu.logic.ts#L241)
- [apps/web/src/hooks/useThreadActionMenu.ts:200-213](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActionMenu.ts#L200-L213)

## projects-navigation-013

New thread on the same branch or worktree requires manual reselection

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Starting another conversation about the same checkout takes an extra workspace-selection step and can accidentally aim work at the main checkout if the user assumes the contextual action preserves the branch.

Recommendation. Offer an explicit New thread in this checkout action that carries the thread's workspace binding. Keep the existing project-level action accurately named.

Acceptance check. From a thread in a non-main worktree, choose New thread in this checkout. The draft names the identical path and branch before send, and the new session starts there.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:453-456](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L453-L456)
- [crates/ferrite-cockpit/src/cockpit.rs:7264-7275](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L7264-L7275)
- [apps/web/src/components/threadActionMenu.logic.ts:123-130](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/threadActionMenu.logic.ts#L123-L130)
- [apps/web/src/hooks/useThreadActionMenu.ts:215-221](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActionMenu.ts#L215-L221)

## projects-navigation-014

Park combines session closure and sidebar settlement, unlike T3

P2. intentional-difference. source-proven. Disposition `product-choice`.

Ferrite has no separate lightweight settled state or inactivity-based settlement control. Tidying a running thread by parking ends its session and can remove group membership. T3 users may expect settlement to be a list-placement action distinct from archive.

Recommendation. Decide explicitly whether Park remains a session-lifecycle operation. If parity is wanted, add a separate settled placement and auto-settle policy. Explain the session effect before treating Park as the equivalent of T3 Settle.

Acceptance check. Document and exercise Settle or Park during a running turn, while grouped, after inactivity and after merge. The observed session and membership behavior must match the labels and chosen product policy.

[Full investigation and reproduction conditions](reports/projects-navigation.md).

Evidence:

- [docs/adr/0040-the-nav-is-the-inbox.md:20-23](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/adr/0040-the-nav-is-the-inbox.md#L20-L23)
- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:475-489](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L475-L489)
- [crates/ferrite-core/src/engine.rs:2276-2278](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L2276-L2278)
- [crates/ferrite-core/src/cockpit.rs:3308-3332](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L3308-L3332)
- [crates/ferrite-cockpit/src/cockpit.rs:8834-8867](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L8834-L8867)
- [apps/web/src/components/threadActionMenu.logic.ts:139-145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/threadActionMenu.logic.ts#L139-L145)
- [apps/web/src/hooks/useThreadActions.ts:384-425](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActions.ts#L384-L425)
- [apps/web/src/components/settings/SettingsPanels.tsx:1872-1944](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsPanels.tsx#L1872-L1944)

## PS-03

Provider version checks and updates target local binaries while a remote server is active

P2. defect. source-proven. Disposition `repair-or-quality`.

A user trying to repair a remote CLI sees the local CLI version and can update the local installation. Ferrite then tells the remote server that its CLI was replaced, causing unnecessary remote session retirement without upgrading that remote CLI.

Recommendation. Route remote provider inspection and upgrades through the selected server, with machine-labelled progress. If remote upgrades remain unsupported, label local controls and never invalidate remote sessions after a local update.

Acceptance check. Use fake local and remote provider executables with distinct versions and update markers. In a remote window, update the selected remote provider and verify only its marker changes and only its sessions retire. A local-only fallback must name this computer and leave remote sessions intact.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:4033-4044](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4033-L4044)
- [crates/ferrite-cockpit/src/cockpit.rs:4083-4115](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4083-L4115)
- [crates/ferrite-cockpit/src/cockpit.rs:12968-12975](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12968-L12975)
- [crates/ferrite-core/src/providers/discover.rs:170-184](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/discover.rs#L170-L184)
- [crates/ferrite-core/src/cockpit.rs:1624-1636](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit.rs#L1624-L1636)
- [crates/ferrite-core/src/providers/update.rs:214-231](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/update.rs#L214-L231)
- [apps/web/src/components/settings/ProviderSettingsPanel.tsx:635](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSettingsPanel.tsx#L635)
- [apps/web/src/components/settings/ProviderSettingsPanel.tsx:657](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSettingsPanel.tsx#L657)

## PS-04

Provider setup stops at CLI presence and update status

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A new user or a user with expired credentials cannot complete provider setup, identify the active account, or retry provider sign-in from Ferrite settings. Existing native CLI authentication can still work.

Recommendation. Add a compact provider readiness/setup panel with the correct machine, detected binary, version, account/auth status where supported, setup instructions or managed install, recheck, and sign-in/retry actions. Preserve externally managed CLI installs.

Acceptance check. Exercise absent CLI, signed-out CLI, expired auth, supported logged-in CLI and CLI version rejection. Each state has a distinct message and working next action. Rechecking reflects changed credentials without starting a paid prompt. Account switching must not leave existing helpers using the old credential binding.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:4743-4753](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4743-L4753)
- [crates/ferrite-cockpit/src/cockpit.rs:4804-4835](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4804-L4835)
- [crates/ferrite-cockpit/src/cockpit.rs:12968-12975](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12968-L12975)
- [crates/ferrite-cockpit/src/cli_updates.rs:240-263](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cli_updates.rs#L240-L263)
- [crates/ferrite-core/src/providers/update.rs:194-218](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/update.rs#L194-L218)
- [crates/ferrite-core/src/providers/codex.rs:184-227](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex.rs#L184-L227)
- [apps/web/src/components/settings/ProviderAuthenticationSection.tsx:43](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderAuthenticationSection.tsx#L43)
- [apps/web/src/components/settings/ProviderAuthenticationSection.tsx:279](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderAuthenticationSection.tsx#L279)
- [apps/web/src/components/settings/ProviderInstanceCard.tsx:750](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderInstanceCard.tsx#L750)
- [apps/web/src/components/settings/ProviderSetupSection.tsx:110](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSetupSection.tsx#L110)
- [apps/server/src/provider/ProviderAuthService.ts:93](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/provider/ProviderAuthService.ts#L93)

## PS-05

An unavailable native conversation has no same-thread context recovery path

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

If a provider's saved conversation disappears or cannot be resumed, the Ferrite transcript can remain readable while sending another prompt repeats the same failed resume. Users must reconstruct the conversation or switch providers through a separate workflow.

Recommendation. Classify resume errors, preserve the failed prompt, and provide an explicit fresh-conversation recovery action with a context preview or clear disclosure. Keep provider-lock contention as a refusal.

Acceptance check. With a provider stub that rejects the old native id, recover within the same Ferrite thread and send the preserved prompt once to a new native id with retained context. The old transcript remains. A thread-in-use refusal must not silently fork. Live-provider behavior remains to be checked.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-core/src/engine.rs:622-625](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L622-L625)
- [crates/ferrite-core/src/engine.rs:1051-1054](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L1051-L1054)
- [crates/ferrite-core/src/engine.rs:2583-2590](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L2583-L2590)
- [crates/ferrite-core/src/engine.rs:2635-2644](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L2635-L2644)
- [crates/ferrite-core/src/engine.rs:3236-3241](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine.rs#L3236-L3241)
- [crates/ferrite-core/src/providers/codex.rs:504-508](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex.rs#L504-L508)
- [apps/server/src/orchestration-v2/ProviderTurnStartService.ts:983](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/ProviderTurnStartService.ts#L983)
- [apps/server/src/orchestration-v2/ProviderTurnStartService.ts:1307](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/ProviderTurnStartService.ts#L1307)

## PS-06

Provider choices do not include T3's additional drivers or named provider instances

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users of Cursor, Grok, OpenCode, Antigravity, Kiro, Muse, Pi, FX or registry ACP agents cannot select those drivers in Ferrite. Users cannot configure separate named instances of one driver with different credentials, environments or binaries.

Recommendation. Decide which additional providers the product actually wants before expanding. If multiple configurations matter first, introduce instance identity separately from driver kind and retain per-driver capability gates.

Acceptance check. For any adopted driver, complete setup, new thread, stream, approval, stop, reopen and failure recovery. For instances, run two named configurations concurrently, restart Ferrite and verify each resumes with its original config. Do not mark an icon or model-list entry as support.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-core/src/store.rs:146-157](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/store.rs#L146-L157)
- [crates/ferrite-core/src/spawner.rs:408-420](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/spawner.rs#L408-L420)
- [crates/ferrite-core/src/providers/discover.rs:170-198](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/discover.rs#L170-L198)
- [crates/ferrite-core/src/settings.rs:28-50](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/settings.rs#L28-L50)
- [crates/ferrite-cockpit/src/cockpit.rs:4485-4504](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4485-L4504)
- [packages/client-runtime/src/providerClients.ts:30](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/client-runtime/src/providerClients.ts#L30)
- [packages/contracts/src/providerInstance.ts:74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/providerInstance.ts#L74)
- [apps/web/src/components/settings/ProviderSettingsPanel.tsx:974](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSettingsPanel.tsx#L974)

## PS-07

Claude Auto and Don't ask are available per session but absent from new-thread defaults

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

An operator who consistently uses Claude Auto or Don't ask must change each new thread or edit JSON/native CLI configuration. Settings cannot express all permission modes that Ferrite already supports.

Recommendation. Build defaults from a provider-specific launch-mode catalog that includes supported Auto and Don't ask, while retaining live-only restrictions for mode changes and a visible representation for unknown saved values.

Acceptance check. Select Auto in Settings through the UI, relaunch and create a Claude thread. The provider receives auto and the setting remains selected. Repeat for dontAsk and a previously saved unsupported value. This is an internal Ferrite inconsistency; T3's unified modes are only a related comparison.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-core/src/providers/claude.rs:533-551](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/claude.rs#L533-L551)
- [crates/ferrite-cockpit/src/cockpit.rs:4580-4595](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4580-L4595)
- [crates/ferrite-core/src/settings.rs:354-361](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/settings.rs#L354-L361)
- [crates/ferrite-core/src/spawner.rs:427-430](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/spawner.rs#L427-L430)
- [apps/web/src/components/settings/ProjectDefaultsSettings.tsx:335](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProjectDefaultsSettings.tsx#L335)

## PS-08

Provider defaults have no project override or fallback-model settings

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Users who work across projects with different model/permission needs must keep changing thread choices or global defaults. A chosen default provider exhausting its allowance does not have a configured fallback model for new threads.

Recommendation. Prioritize project defaults only if repeated manual changes are a real workflow. Use a small inheritance model and show the resolved source. Treat allowance-triggered fallback as a separate opt-in policy.

Acceptance check. Configure two projects with different defaults, create a thread in each and verify the right provider/model/permission after restart. Existing threads retain their own choices. A synthetic exhausted allowance selects only the configured fallback for new threads and visibly explains that selection.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-core/src/settings.rs:28-50](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/settings.rs#L28-L50)
- [crates/ferrite-core/src/workspace/registry.rs:35-51](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/workspace/registry.rs#L35-L51)
- [crates/ferrite-core/src/workspace/registry.rs:87-95](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/workspace/registry.rs#L87-L95)
- [crates/ferrite-cockpit/src/cockpit.rs:4485-4566](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L4485-L4566)
- [apps/web/src/components/settings/ProjectDefaultsSettings.tsx:250](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProjectDefaultsSettings.tsx#L250)
- [apps/web/src/components/settings/ProjectDefaultsSettings.tsx:335](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProjectDefaultsSettings.tsx#L335)
- [apps/web/src/components/settings/SettingsScopeContext.tsx:58](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsScopeContext.tsx#L58)

## PS-10

Remote access supports SSH, not T3's pairing links and alternate routes

P2. intentional-difference. source-proven. Disposition `product-choice`.

A machine reachable through a T3 pairing URL or relay cannot be added through Ferrite's machine settings. Ferrite also expects ferrited to be installed on the remote PATH before connection.

Recommendation. Do not describe remote support as absent. Document the existing SSH prerequisite and decide whether pairing/relay or assisted bootstrap is needed. Keep these optional expansions separate from the real remote settings/update defects.

Acceptance check. For present scope, a configured SSH target reconnects and names an absent daemon clearly. If alternate routes are added, pair once, force direct-route failure and verify failover preserves machine/thread identity and requires no duplicate environment.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-core/src/machine.rs:1-7](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/machine.rs#L1-L7)
- [crates/ferrite-core/src/machine.rs:80-128](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/machine.rs#L80-L128)
- [crates/ferrite-core/src/machine.rs:379-385](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/machine.rs#L379-L385)
- [crates/ferrite-cockpit/src/cockpit/machines.rs:354-385](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/machines.rs#L354-L385)
- [docs/adr/0026-machines-over-ssh.md:8-29](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/adr/0026-machines-over-ssh.md#L8-L29)
- [apps/web/src/components/settings/AddEnvironmentDialog.tsx:635](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AddEnvironmentDialog.tsx#L635)
- [apps/web/src/components/settings/AddEnvironmentDialog.tsx:438](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AddEnvironmentDialog.tsx#L438)

## quality-correction-003

Empty browser address entry lacks a clear editable-region affordance

P2. interaction. runtime-reproduced. Disposition `repair-or-quality`.

The empty location row reads as navigation icons, a static Address label, blank space and a separate arrow. The editable region has no prompt; the label and annotation control also spend address width.

Recommendation. Design one location control with explicit empty, idle, hover, focus, editing, loading and invalid states while preserving useful one-line address space.

Acceptance check. An empty tab clearly indicates where to enter a URL before interaction. Hover and keyboard focus identify the field; text and navigation remain usable at supported widths. Verify these native states individually.

[Full investigation and reproduction conditions](reports/quality-correction.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:180-184](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L180-L184)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1452-1488](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1452-L1488)
- [crates/ferrite-pane/src/composer.rs:1084-1136](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L1084-L1136)
- [apps/web/src/components/preview/PreviewChromeRow.tsx:153-215](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L153-L215)
- [apps/web/src/components/ui/input-group.tsx:8-27](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/input-group.tsx#L8-L27)

Review note. Only empty presentation is directly observed here. Native hover/focus/narrow-width behavior remains unverified. T3 also uses a transparent idle field, so absence of a permanent border is not itself the defect.

Runtime evidence:

- [visual-quality-correction.md](reports/visual-quality-correction.md)
- [13-browser-after-files.png](evidence/panel-correction/13-browser-after-files.png)

## quality-correction-004

Native PR read errors run beyond the panel and hide their cause and recovery text

P2. defect. runtime-reproduced. Disposition `repair-or-quality`.

A long PR read error is centered on one unwrapped line wider than the panel. Its beginning and recovery instruction are clipped, leaving the operator unable to read the failure.

Recommendation. Render read failures in a width-constrained, wrapping body with a visible relevant recovery action. Preserve the current PR identity and useful previous content.

Acceptance check. Force a long read error at ordinary/minimum panel widths. The full cause and next action remain readable and reachable; retry recovers without losing target identity.

[Full investigation and reproduction conditions](reports/quality-correction.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:452-469](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L452-L469)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:867-879](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L867-L879)

Review note. Parent inspected the screenshot and the centered pr_note render path. The nonexistent synthetic repository caused this expected read error; it is not evidence that normal repositories fail.

Runtime evidence:

- [pr-entry-runtime-correction.md](reports/pr-entry-runtime-correction.md)
- [04-overview-native-pr.png](evidence/pr-entry-correction/04-overview-native-pr.png)

## quality-correction-005

Thread context lacks an independent compact details surface

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Thread context lives in Overview inside the full tabbed workspace panel, so checking it competes with Browser, Files, PR and other tools. Existing context controls do not provide a separate lightweight card alongside the active tool. This is distinct from remembered-width corruption.

Recommendation. Provide independently accessible lightweight thread details, with separate visibility and responsive popover behavior, while preserving the active full tool surface and conversation reading space. Link agents to their main child conversations and Git/PR actions to explicit full-tool targets.

Acceptance check. Keep Browser, Files or PR open while showing and dismissing thread details. Pointer and Command-Shift-B target only details; Escape returns focus without closing the tool. Exercise narrow-size fallback, overflow, Find clearance, thread identity, live status, meaningful motion and reduced motion.

[Full investigation and reproduction conditions](reports/quality-correction.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel.rs:657-670](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L657-L670)
- [crates/ferrite-cockpit/src/cockpit/panel/sections.rs:1-39](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/sections.rs#L1-L39)
- [apps/web/src/components/chat/ThreadDetailsCard.tsx:149-168](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadDetailsCard.tsx#L149-L168)
- [apps/web/src/components/chat/threadDetailsCardLayout.ts:4-45](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/threadDetailsCardLayout.ts#L4-L45)

Review note. The inventory source links use the pinned newer comparison checkout. reports/thread-details-card-correction.md separately cites the exact installed revision and records native T3 identity, toggle, shortcut, one responsive mode transition, empty-workspace coexistence and Escape checks. Those T3 successes do not upgrade this Ferrite composition gap to runtime-reproduced. This finding was omitted from the earlier 176-record tally.

## quote-attachments-002

Quote previews hide their full contents with no disclosure

P2. interaction. runtime-reproduced. Disposition `repair-or-quality`.

The user cannot inspect the whole passage they are about to cite, and a sent prompt cannot reveal the complete quote or a preloaded long comment. Similar openings become indistinguishable.

Recommendation. Use a text-sized citation row or chip with full inspection and comment editing. Preserve compactness through disclosure rather than irreversible clipping. Restore provenance under transcript-006; this finding separately covers local readability even when a source is unavailable.

Acceptance check. A 5,000-character citation, two citations with identical opening words, a code excerpt and a multiline comment can each be inspected in full before and after send. The UI clearly distinguishes source text from comment text at narrow pane widths. Screenshot review confirms readable wrapping and controls.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/attachments.rs:505-543](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L505-L543)
- [crates/ferrite-ui/src/theme.rs:756-757](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/theme.rs#L756-L757)
- [crates/ferrite-pane/src/pane.rs:3528-3533](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L3528-L3533)
- [apps/web/src/components/chat/AssistantCitationChip.tsx:87](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L87)
- [apps/web/src/components/chat/AssistantCitationChip.tsx:106](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L106)
- [apps/web/src/components/chat/AssistantCitationChip.tsx:255](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L255)

Review note. A roughly 991-character response attached as context was truncated in the 88px tile and could not be inspected by clicking it. Full prose/code still reached the provider. This does not test the >10,000-character attachment limit.

Runtime evidence:

- [attachment-runtime-correction.md](reports/attachment-runtime-correction.md)
- [13-whole-response-cited.png](evidence/attachment-correction/13-whole-response-cited.png)
- [14-tile-click.png](evidence/attachment-correction/14-tile-click.png)
- [provider-host.jsonl](evidence/attachment-correction/provider-host.jsonl)

## quote-attachments-003

The Cite toolbar is pointer-only and does not observe keyboard selection

P2. interaction. source-proven. Disposition `repair-or-quality`.

A keyboard user cannot reach or activate the displayed Cite action. A selection changed without mouse release does not create a fresh offer.

Recommendation. Give Cite a focusable native control and explicit keyboard activation while retaining a captured selection until activation settles. Observe selection changes independently of mouse-up. Keep Escape dismissal and the oversize explanation.

Acceptance check. Select a passage, Tab to Cite, activate with Enter and with Space, and confirm exactly one citation and no message send. Keyboard-only selection exposes the same action. Escape restores a predictable focus target without interrupting the agent.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/transcript.rs:3193-3195](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L3193-L3195)
- [crates/ferrite-pane/src/transcript/reply.rs:59-96](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/reply.rs#L59-L96)
- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:140](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L140)
- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:148](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L148)
- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:191](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L191)

## quote-attachments-004

Attach message as context silently does nothing for long messages

P2. defect. source-proven. Disposition `repair-or-quality`.

The visible attach action can produce no attachment, error or explanation for a long assistant answer. This is a concrete no-op that can resemble the reported citation regression.

Recommendation. Use a whole-message source reference, or show a disabled reason and offer a shorter selected passage. A successful-looking action must not disappear silently.

Acceptance check. Exercise 1, 2, 10,000 and 10,001 trimmed characters through the actual menu. Each action either creates context or displays an explicit reason. Unicode bounds match the explanation.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/transcript/message_menu.rs:103-123](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L103-L123)
- [crates/ferrite-pane/src/quote.rs:13-26](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L13-L26)
- [crates/ferrite-pane/src/transcript/reply.rs:225-227](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/reply.rs#L225-L227)
- [apps/web/src/components/chat/TimelineAssistantRows.tsx:220](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineAssistantRows.tsx#L220)
- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:179](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L179)

## quote-attachments-005

Ordinary blockquote text is reclassified as an uneditable citation on restore

P2. defect. source-proven. Disposition `repair-or-quality`.

An ordinary prompt can change into attachment cards when reopened or recalled. Its text leaves the main editor, and its editing affordances disappear. This is a structure and editability defect; the words usually remain serialized.

Recommendation. Persist typed citation records or an unambiguous versioned encoding. Keep literal Markdown literal. Preserve multiline comments and support legacy prompt decoding without treating every new leading blockquote as structured context.

Acceptance check. Round-trip literal leading blockquotes, nested quotes, the literal ↩ marker, blank lines, Unicode and deliberate citations through draft restart, history and resend. Compare both text and editable record types. General text remains in the general editor.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/quote.rs:67-118](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L67-L118)
- [crates/ferrite-pane/src/composer.rs:494-512](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L494-L512)
- [crates/ferrite-cockpit/src/cockpit/drafts.rs:28-56](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/drafts.rs#L28-L56)
- [crates/ferrite-cockpit/src/cockpit.rs:6641-6649](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L6641-L6649)
- [packages/shared/src/assistantCitations.ts:21](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L21)
- [packages/shared/src/assistantCitations.ts:43](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L43)
- [packages/shared/src/assistantCitations.ts:118](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L118)

## quote-attachments-006

A quote-only prompt renders differently when a file is attached

P2. defect. source-proven. Disposition `repair-or-quality`.

Adding an unrelated file can turn the citation into raw > text in the sent prompt. Draft restoration reconstructs the quote differently from transcript rendering. Imported comments can also absorb the attachment trailer and hide the file preview.

Recommendation. Decode a prompt once into text, quotes, files and annotations, then render that result. Do not repeatedly split and recompose transport syntax inside render helpers.

Acceptance check. Test the cross-product of zero or nonempty main text, one or several quotes, zero or multiline comments, file/image attachments and browser annotations. Sent, reopened and recalled forms retain the same quote rows, file previews and comment boundaries. No transport token appears as comment prose.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/quote.rs:74-85](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L74-L85)
- [crates/ferrite-core/src/prompt_files.rs:15-24](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/prompt_files.rs#L15-L24)
- [crates/ferrite-core/src/browser_annotations.rs:475-478](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/browser_annotations.rs#L475-L478)
- [crates/ferrite-pane/src/composer.rs:506-511](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L506-L511)
- [crates/ferrite-pane/src/pane.rs:3226-3241](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L3226-L3241)
- [apps/web/src/components/chat/useComposerSend.ts:1292](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1292)
- [apps/web/src/components/chat/TimelineUserMessageRow.tsx:483](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L483)

## quote-attachments-008

Valid image attachments can show a permanent false Failed tile

P2. defect. source-proven. Disposition `repair-or-quality`.

An image supported by the rendering library can appear broken before it is decoded. A repaired or newly created file can remain Failed until the app process restarts.

Recommendation. Let the supported decoder determine displayability. Use a neutral dimension fallback while metadata is unknown. Key cached facts by a file version or explicit attachment identity and expose a retry after changes. This is separate from composer-07's provider readiness problem.

Acceptance check. Valid SVG, AVIF, TIFF, PNG and a JPEG with large metadata each show a preview or an accurate format-specific explanation. A missing/corrupt file repaired at the same path recovers without restarting. Replacing an image updates dimensions and caption.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/image_viewer.rs:65-72](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/image_viewer.rs#L65-L72)
- [vendor/gpui-pre/src/elements/img.rs:210-217](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/img.rs#L210-L217)
- [crates/ferrite-pane/src/attachments.rs:451-498](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L451-L498)
- [crates/ferrite-pane/src/pane.rs:3861-3896](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L3861-L3896)
- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:220](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L220)
- [apps/web/src/components/chat/useComposerAttachmentIntake.ts:273](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L273)

## quote-attachments-009

Draft file previews and sent image previews lack keyboard activation

P2. interaction. source-proven. Disposition `repair-or-quality`.

A keyboard user can remove a draft file but cannot activate its preview. Image preview is keyboard-capable in the draft, then loses that capability after the message sends.

Recommendation. Share a semantic preview control between draft and transcript renderers, with a stable accessible name and focus restoration when preview closes. Keep removal a separate control.

Acceptance check. Open and close text and image previews entirely by keyboard before and after send. Every activation opens once without sending a message. Tab exposes preview and remove as distinct labeled actions. An accessibility inspection sees buttons rather than anonymous clickable text.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/attachments.rs:191-257](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L191-L257)
- [crates/ferrite-pane/src/pane.rs:3800-3838](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L3800-L3838)
- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:470](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L470)
- [apps/web/src/components/chat/TimelineUserMessageRow.tsx:733](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L733)

## quote-attachments-010

Attachment history stores mutable paths rather than retained content

P2. intentional-difference. source-proven. Disposition `product-choice`.

Retry and history can refer to changed or deleted file content. A pasted screenshot depends on a temporary file even after its prompt has been saved. The visual record is not a reliable record of the bytes originally sent. ADR 0004 explicitly chooses path storage, so this is a documented product tradeoff with incomplete user feedback, not an accidental omission of a promised byte store.

Recommendation. Distinguish live file references from retained attachments. Copy clipboard media and explicit attachment snapshots into an owned durable store and keep content identity with history. Detect stale references and expose reattach/retry instead of silently changing content. Keep workspace @mentions live if that is the intended contract.

Acceptance check. Send version A of a fixture, replace the original with B, reopen and inspect history, then retry. The UI and provider payload agree on A versus B and never silently substitute. A pasted screenshot survives restart and routine temp cleanup, or displays an actionable missing-asset state before send.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/composer.rs:572-582](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L572-L582)
- [crates/ferrite-core/src/prompt_files.rs:15-24](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/prompt_files.rs#L15-L24)
- [crates/ferrite-cockpit/src/cockpit/drafts.rs:28-56](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/drafts.rs#L28-L56)
- [crates/ferrite-pane/src/pane.rs:3800-3836](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L3800-L3836)
- [crates/ferrite-pane/src/preview.rs:271-290](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/preview.rs#L271-L290)
- [crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:169-190](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/transcript_glue.rs#L169-L190)
- [crates/ferrite-core/src/providers/claude/wire.rs:28-62](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/claude/wire.rs#L28-L62)
- [crates/ferrite-core/src/providers/codex/wire.rs:1276-1293](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/providers/codex/wire.rs#L1276-L1293)
- [docs/adr/0004-prompt-attachments.md:23-36](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/adr/0004-prompt-attachments.md#L23-L36)
- [apps/web/src/components/chat/useComposerAttachmentIntake.ts:204](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L204)
- [apps/web/src/lib/attachmentUploadQueue.ts:135](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135)
- [apps/web/src/composerDraftStore.ts:2291](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2291)
- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:462](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L462)

## quote-attachments-011

The attachment shelf has no bounded overflow policy

P2. interaction. inferred. Disposition `repair-or-quality`.

At narrow pane widths, many attachments can consume the available conversation height. Source shows unbounded growth; the installed app must establish the exact clipping or displacement symptom.

Recommendation. Measure the installed layout with many attachments, then provide a bounded attachment region or compact summary with explicit expansion. Preserve navigation, removal and the active editor. Do not copy T3's expanded wrapping blindly; its resting summary and screenshot horizontal scrolling are the relevant comparison.

Acceptance check. At 360 pixel pane width and 600 pixel window height, add 12 images, six citations and long-named files. The editor, Send and all attachments remain reachable by pointer and keyboard. Resize and remove items without losing focus or the visible edit position. Capture before/after screenshots.

[Full investigation and reproduction conditions](reports/quote-attachments.md).

Evidence:

- [crates/ferrite-pane/src/attachments.rs:275-295](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/attachments.rs#L275-L295)
- [crates/ferrite-pane/src/composer.rs:572-582](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L572-L582)
- [crates/ferrite-pane/src/pane/composer.rs:501-510](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L501-L510)
- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:179](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L179)
- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:583](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L583)
- [apps/web/src/components/composerFooterLayout.ts:1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/composerFooterLayout.ts#L1)

## scroll-quality-001

Scrollbar navigation does not consistently take ownership from automatic scrolling

P2. defect. source-proven. Disposition `repair-or-quality`.

Track clicks during a send/latest ease can be overwritten by the next animation frame. Thumb movement after the ease can leave the sent-prompt anchor active, allowing the first tool event to snap back to the tail if runway remains. Extends the motion report's interruption inventory; does not duplicate motion-008.

Recommendation. Emit a common reader-navigation transition from both scrollbar track clicks and thumb movement, independently of whether a tween exists. Cancel pending send anchoring before applying the user offset; preserve explicit drag-to-end reattachment.

Acceptance check. Real track clicks and pointer drags interrupt movement at every tween phase. Repeat a short thumb drag after send settlement with runway remaining, then emit the first tool event. The chosen passage stays put after later frames/events; dragging to the bottom still resumes follow.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [crates/ferrite-ui/src/scrollbar.rs:871-905](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/scrollbar.rs#L871-L905)
- [vendor/gpui-base/src/scrollbar.rs:125-142](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/scrollbar.rs#L125-L142)
- [crates/ferrite-pane/src/transcript/scroll.rs:163-169](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L163-L169)
- [vendor/gpui-pre/src/elements/list.rs:1371-1416](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/list.rs#L1371-L1416)
- [crates/ferrite-pane/src/transcript.rs:1033-1055](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1033-L1055)
- [apps/web/src/components/chat/timelineViewport.ts:91-100](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.ts#L91-L100)
- [apps/web/src/components/chat/useTimelineFollow.ts:238-248](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L238-L248)
- [apps/web/src/components/chat/timelineViewport.browser.test.tsx:83](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.browser.test.tsx#L83)

## scroll-quality-002

Scrollbar track clicks use the wrong scroll distance

P2. defect. source-proven. Disposition `repair-or-quality`.

Track clicks overshoot the position corresponding to the painted thumb and clamp to the end early because the click branch multiplies by content height instead of content height minus viewport height.

Recommendation. Share one inverse coordinate conversion across thumb painting, dragging and track clicks. Handle insets and minimum thumb size explicitly, or define an intentional page-by-page track policy.

Acceptance check. For fixed content and viewport sizes, clicking 25%, 50% and 75% of effective thumb travel centers the thumb at the clicked position within a specified tolerance. Repeat with minimum-size and horizontal thumbs. The 2000px content/500px viewport/0.5 fraction arithmetic yields 750px of scroll, not 1000px.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [crates/ferrite-ui/src/scrollbar.rs:544-554](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/scrollbar.rs#L544-L554)
- [vendor/gpui-base/src/scrollbar.rs:133-134](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/scrollbar.rs#L133-L134)
- [crates/ferrite-ui/src/components.rs:1904-1908](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/components.rs#L1904-L1908)
- [apps/web/src/components/chat/MessagesTimeline.tsx:1471-1476](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L1471-L1476)

## scroll-quality-003

The latest button undercounts hidden content when send runway remains

P2. defect. source-proven. Disposition `repair-or-quality`.

The latest button and lower fade can be absent while real answer lines remain below the viewport. rows_below subtracts normal padding and synthetic runway from a native maximum that already excludes padding.

Recommendation. Separate real content-end distance from padded scrollable-end distance, and audit consumers against the native list's actual treatment of padding and unmeasured heights.

Acceptance check. With at least 200px of runway, position more than two lines of real content below the viewport. After the configured delay, the latest button and bottom fade are present. Latest removes the runway and reaches the final row. Repeat with no runway and with unmeasured rows.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [vendor/gpui-pre/src/elements/list.rs:772-795](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/list.rs#L772-L795)
- [crates/ferrite-pane/src/transcript.rs:1063-1070](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1063-L1070)
- [apps/web/src/components/chat/MessagesTimeline.logic.ts:319-336](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.logic.ts#L319-L336)
- [apps/web/src/components/chat/useTimelineFollow.ts:305-325](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L305-L325)

## scroll-quality-004

Expanding finished work at the tail does not preserve the clicked heading

P2. interaction. source-proven. Disposition `repair-or-quality`.

A completed fold or large tool detail expands while native tail follow remains enabled, moving the heading upward and possibly out of view. This concerns viewport ownership, distinct from motion.md inventory row 37 body animation and transcript-004 nested group height caps.

Recommendation. Capture the clicked disclosure's anchor before changing rows, cancel prior positioning, and reveal new content after measurement while retaining the heading. Define the working-turn follow exception explicitly.

Acceptance check. At an idle tail, opening and closing a fold hiding several viewports leaves its heading at the same screen position. Repeat a long tool detail near the composer, expansion during a latest jump, rapid reversal, and a separately specified live-streaming case.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [crates/ferrite-pane/src/transcript.rs:1364-1370](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1364-L1370)
- [crates/ferrite-cockpit/src/cockpit.rs:2108-2114](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L2108-L2114)
- [vendor/gpui-pre/src/elements/list.rs:1043-1048](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/list.rs#L1043-L1048)
- [crates/ferrite-pane/src/transcript/scroll.rs:311-324](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L311-L324)
- [apps/web/src/components/chat/MessagesTimeline.tsx:535-573](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L535-L573)
- [apps/web/src/components/chat/MessagesTimeline.tsx:492-523](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L492-L523)
- [apps/web/src/components/chat/MessagesTimeline.browser.test.tsx:298-340](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.browser.test.tsx#L298-L340)

## scroll-quality-005

Keyboard End retains the send spacer that the latest button removes

P2. defect. source-proven. Disposition `repair-or-quality`.

End can retain a large blank area below the newest message; during runway sizing a later layout callback can reassert the prompt position. The latest button clears this state, so explicit return-to-end actions disagree.

Recommendation. Route every explicit return-to-end action through one state transition that releases send anchoring, pending runway placement and synthetic space before landing. Let the caller choose timing separately.

Acceptance check. End and the latest pill from identical send states leave zero runway, the same final-row inset, and active following. End during the first sizing frame is not overwritten by a later runway callback. Repeat with reduced motion.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [crates/ferrite-pane/src/transcript.rs:1073-1091](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1073-L1091)
- [crates/ferrite-pane/src/transcript/scroll.rs:111-114](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L111-L114)
- [crates/ferrite-cockpit/src/cockpit.rs:2069-2076](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L2069-L2076)
- [apps/web/src/components/chat/useTimelineFollow.ts:305-325](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L305-L325)

## scroll-quality-006

Find can compete with an existing jump or live follow inside a visible row

P2. defect. source-proven. Disposition `repair-or-quality`.

Opening Find does not cancel follow/ease. Revealing a match only cancels when its entire row is offscreen; a tall intersecting row can let the old tween or stream move away from the result. Distinct from transcript-001 hidden-content search coverage.

Recommendation. Treat Find navigation as a viewport-ownership transition regardless of whole-row visibility. Scope later reveal callbacks to the search request and transcript generation, and let manual navigation cancel them.

Acceptance check. Find a result in a tall row halfway through a latest jump. The occurrence remains visible after the old tween would have ended and after later text/tool events. Repeat for a match already visible while following. Manual navigation after searching must win over delayed reveal corrections.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [crates/ferrite-pane/src/transcript/find.rs:40-51](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/find.rs#L40-L51)
- [crates/ferrite-pane/src/transcript/scroll.rs:229-233](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L229-L233)
- [crates/ferrite-pane/src/transcript.rs:2921-2933](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2921-L2933)
- [apps/web/src/components/chat/MessagesTimeline.browser.test.tsx:1737-1819](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.browser.test.tsx#L1737-L1819)
- [apps/web/src/components/chat/useTimelinePositionRestore.ts:84-99](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L84-L99)

## scroll-quality-007

Width changes lack a reading anchor inside a tall row

P2. partial-capability. inferred. Disposition `repair-or-quality`.

A right-panel toggle or resize can change which sentence is visible when the viewport lies entirely inside a long answer. Ferrite saves only a fully visible row start and otherwise retains a numeric pixel offset through rewrapping. Displacement is inferred and needs runtime measurement; T3 sentence-level preservation is also unverified.

Recommendation. Preserve a text/sub-row anchor for tall content, while retaining row-inset anchoring when sufficient. Specify the fallback for content without a text anchor and verify both implementations before claiming parity superiority.

Acceptance check. A distinctive sentence in a multi-viewport answer remains at a documented readable inset across panel open/close/maximize/restore and width changes. Repeat with a code fence and a visible row boundary. Confirm on the installed build before labeling it runtime-reproduced.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [crates/ferrite-pane/src/transcript/scroll.rs:238-281](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L238-L281)
- [vendor/gpui-pre/src/elements/list.rs:412-425](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/list.rs#L412-L425)
- [crates/ferrite-pane/src/transcript.rs:3147-3158](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L3147-L3158)
- [apps/web/src/components/chat/MessagesTimeline.tsx:909-912](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L909-L912)
- [apps/web/src/components/chat/useTimelinePositionRestore.ts:105-144](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L105-L144)

## scroll-quality-008

Saved jump, runway and resize indices are not rebased when rows change

P2. defect. inferred. Disposition `repair-or-quality`.

Pending movement can use an obsolete numeric row index after disclosure changes, retention or history replacement. Native logical top is rebased, but EaseTarget::Row, Runway.index and resize_anchor are separate and are not rebased/reset together. The omission is source-proven; a wrong-row landing remains an inferred concurrent-transition outcome.

Recommendation. Store semantic RowId plus transcript generation for pending targets, resolve after each projection, and cancel missing targets. Reset or deliberately translate all geometry state on history replacement.

Acceptance check. Insert/remove rows before a moving send target and verify it still lands on the same prompt ID. Repeat with width reflow, retention, target removal and generation replacement. Returning from another thread never revives a command against a different history generation.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [crates/ferrite-pane/src/transcript/scroll.rs:18-24](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L18-L24)
- [crates/ferrite-pane/src/transcript.rs:492-497](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L492-L497)
- [vendor/gpui-pre/src/elements/list.rs:537-547](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-pre/src/elements/list.rs#L537-L547)
- [crates/ferrite-pane/src/pane.rs:355-361](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L355-L361)
- [crates/ferrite-cockpit/src/cockpit.rs:2493-2499](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L2493-L2499)
- [apps/web/src/components/chat/useTimelineFollow.ts:48-97](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L48-L97)
- [apps/web/src/components/chat/useTimelineFollow.ts:374-423](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L374-L423)
- [apps/web/src/components/chat/useTimelinePositionRestore.ts:124-144](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L124-L144)
- [apps/web/src/components/chat/timelineScrollAnchoring.test.tsx:19-66](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineScrollAnchoring.test.tsx#L19-L66)

## scroll-quality-009

Page navigation does not subtract the pinned prompt's covered area

P2. defect. source-proven. Disposition `repair-or-quality`.

Page movement uses full viewport height minus one line even though the pinned prompt obscures the top. When the mask exceeds that line, content can be hidden on both adjacent pages. Existing keyboard tests verify offsets and follow state, not readable coverage.

Recommendation. Compute page travel from the unobscured viewport using the measured top mask, and retain a readable overlap. Define the transition when the pinned prompt changes height.

Acceptance check. Page up/down through numbered answer lines under a two-line pinned prompt. Every line must appear unobscured and adjacent pages overlap. Repeat with a short prompt, no pinned prompt, and changing prompt height.

[Full investigation and reproduction conditions](reports/scroll-quality.md).

Evidence:

- [crates/ferrite-pane/src/transcript/scroll.rs:128-133](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L128-L133)
- [crates/ferrite-pane/src/transcript.rs:1081-1087](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1081-L1087)
- [crates/ferrite-ui/src/theme.rs:1928](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/theme.rs#L1928)
- [crates/ferrite-pane/src/transcript/find.rs:167-171](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/find.rs#L167-L171)
- [apps/web/src/components/chat/pageScrollController.ts:115-120](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/pageScrollController.ts#L115-L120)

## subagents-background-01

Opening an evicted agent through its transcript row never requests saved history

P2. defect. source-proven. Disposition `repair-or-quality`.

A real agent can open as an empty transcript despite its history existing in Ferrite storage.

Recommendation. Make the Panel agent opener request and observe saved history, render loading/error/coverage states, and support retry through the same loader as full-Pane navigation.

Acceptance check. With a small child-retention limit, finish and evict agent A, then click A in Main. Its stored prompt and response load without resuming its provider. A failing read shows a visible retry that succeeds after the storage failure clears.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:80-110](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L80-L110)
- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:154-163](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L154-L163)
- [crates/ferrite-cockpit/src/cockpit/side_chat.rs:492-502](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L492-L502)
- [crates/ferrite-cockpit/src/cockpit/subagents.rs:397-400](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L397-L400)
- [crates/ferrite-cockpit/src/cockpit/subagents.rs:1055-1069](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L1055-L1069)
- [crates/ferrite-core/src/engine/history.rs:110-172](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/engine/history.rs#L110-L172)
- [crates/ferrite-core/src/activity.rs:330-336](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/activity.rs#L330-L336)
- [apps/web/src/components/chat/useThreadLifecycle.tsx:466-495](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L466-L495)

Review note. The defect requires an unretained child. The default retains 128 children. The source condition is established, but its frequency and the installed restart path were not reproduced.

## subagents-background-04

Agent Panel views omit coverage, decision guidance and full status

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A partial, waiting, failed, or unavailable child can look like an ordinary read-only transcript with no explanation of what the operator should do.

Recommendation. Give the primary agent drill an identity/status header, coverage explanation, and an explicit route to its pending request. Preserve read-only prompting semantics. State whether Stop targets this child.

Acceptance check. Open agents in Pending, Waiting, Failed, NotLoaded, Partial, and ToolActivity states. Each view states its true status/coverage. Waiting exposes a route to the exact request; opening and answering it preserves the parent draft.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:145-175](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L145-L175)
- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-225](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L203-L225)
- [crates/ferrite-cockpit/src/cockpit/side_chat.rs:477-482](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L477-L482)
- [crates/ferrite-cockpit/src/cockpit/side_chat.rs:558-565](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/side_chat.rs#L558-L565)
- [crates/ferrite-cockpit/src/cockpit/panel/strip.rs:249-260](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/strip.rs#L249-L260)
- [crates/ferrite-cockpit/src/cockpit/subagents.rs:350-367](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L350-L367)
- [crates/ferrite-cockpit/src/cockpit/subagents.rs:459-604](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L459-L604)
- [apps/web/src/components/chat/ProviderSubagentBar.tsx:14-75](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14-L75)
- [apps/web/src/components/chat/SubagentTooltipContent.tsx:33-61](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SubagentTooltipContent.tsx#L33-L61)
- [apps/web/src/components/ChatView.tsx:1746-1748](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatView.tsx#L1746-L1748)

## subagents-background-05

Generic thread opening can replace Main with the first pending child

P2. interaction. source-proven. Disposition `repair-or-quality`.

A user selecting a thread can unexpectedly land in a background agent and lose the visible Main composer, while a direct agent-row click instead opens a side tab.

Recommendation. Separate Open thread from Open request, and visibly name child destinations on request actions. Use one explicit inspection/follow policy; do not silently retarget generic thread opening to whichever decision happens to be first.

Acceptance check. With Main and two children awaiting different approvals, activate the generic thread card, then each exact request. The generic action opens Main; request actions name and open the selected owner. Back returns to the prior place and Main draft remains unchanged.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit.rs:12274-12285](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12274-L12285)
- [crates/ferrite-cockpit/src/cockpit.rs:12298-12327](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L12298-L12327)
- [crates/ferrite-cockpit/src/cockpit/subagents.rs:415-438](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L415-L438)
- [crates/ferrite-cockpit/src/cockpit/agent_pane.rs:1-10](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/agent_pane.rs#L1-L10)
- [crates/ferrite-cockpit/src/cockpit/subagents.rs:463-490](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/subagents.rs#L463-L490)
- [apps/web/src/components/chat/ProviderSubagentBar.tsx:63-75](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L63-L75)
- [apps/web/src/components/chat/useThreadLifecycle.tsx:466-495](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L466-L495)

## subagents-background-06

Background Stop never clears its pending marker or offers retry after failure

P2. defect. source-proven. Disposition `repair-or-quality`.

A rejected or ineffective stop can leave Stopping... permanently disabled. Reused task IDs can inherit this state across threads or session generations.

Recommendation. Key stop requests by thread, session generation and task; clear on failure/completion, expose failures beside Stop, and allow retry if the provider does not settle.

Acceptance check. Reject a Stop request. Its error appears beside the background line and Stop becomes enabled again. Reuse the same native task ID in another thread and a later session and confirm no inherited pending marker.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/background_work.rs:123-128](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/background_work.rs#L123-L128)
- [crates/ferrite-cockpit/src/cockpit/background_work.rs:183-199](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/background_work.rs#L183-L199)
- [crates/ferrite-cockpit/src/cockpit/background_work.rs:206-224](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/background_work.rs#L206-L224)
- [crates/ferrite-cockpit/src/cockpit.rs:598](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L598)
- [crates/ferrite-cockpit/src/cockpit.rs:11561-11578](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L11561-L11578)
- [crates/ferrite-cockpit/src/cockpit.rs:11739-11747](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L11739-L11747)
- [apps/web/src/components/chat/useThreadLifecycle.tsx:416-463](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L416-L463)

## subagents-background-07

Background-work Stop excludes every subagent it counts

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A mixed commands-and-agents line offers Stop but stops only commands; an agents-only line has no Stop even where child interruption is supported.

Recommendation. Make Stop scope explicit and use the existing child-stop operation for stoppable child work. If one action cannot stop everything, give per-item controls and name the remaining work.

Acceptance check. Run one command and one stoppable child after Main settles. Stop either stops both, or clearly says Stop commands and leaves a named child Stop control. An agents-only state offers child Stop where supported.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/background_work.rs:82-93](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/background_work.rs#L82-L93)
- [crates/ferrite-cockpit/src/cockpit/background_work.rs:123-128](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/background_work.rs#L123-L128)
- [crates/ferrite-cockpit/src/cockpit/background_work.rs:206-222](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/background_work.rs#L206-L222)
- [crates/ferrite-cockpit/src/cockpit.rs:5707-5727](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L5707-L5727)
- [apps/web/src/components/chat/ThreadActivityBanners.tsx:170-179](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadActivityBanners.tsx#L170-L179)
- [apps/web/src/components/chat/useThreadLifecycle.tsx:434-443](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L434-L443)

## subagents-background-08

Background task views disagree about which work exists

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

The transcript can say Waiting on a subagent while the Background tab is empty; users must learn three different background-work views.

Recommendation. Use one attributed work inventory for the transcript summary, detail dialog and Background tab. Include agents and shell tasks, retain owner identity, and distinguish awaited work from work merely running. Make palette naming identify the destination.

Acceptance check. With an agents-only workload, the transcript count, Details and Background tab show the same agent. With a command known only from a progress snapshot, every view includes it. A dev server running after Main completes reads Running unless the provider says it is awaited.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/background_work.rs:28-97](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/background_work.rs#L28-L97)
- [crates/ferrite-cockpit/src/cockpit/panel/background.rs:29-39](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/background.rs#L29-L39)
- [crates/ferrite-cockpit/src/cockpit/panel/background.rs:84-94](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/background.rs#L84-L94)
- [crates/ferrite-cockpit/src/cockpit/palette.rs:1273-1276](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/palette.rs#L1273-L1276)
- [crates/ferrite-cockpit/src/cockpit/background_work.rs:345-379](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/background_work.rs#L345-L379)
- [apps/web/src/components/chat/ThreadActivityBanners.tsx:180-208](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadActivityBanners.tsx#L180-L208)
- [apps/web/src/components/chat/BackgroundWorkDetails.tsx:39-63](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L39-L63)

Review note. Clicking an agent name opened its updated child transcript, but that same row’s View live output opened Background with No output yet. This is a reproduced branch of the inconsistent work/output model; other inventory branches retain source-only status.

Runtime evidence:

- [agent-live-correction.md](reports/agent-live-correction.md)
- [06-agent-open-live-two-nested.png](evidence/agent-live-correction/06-agent-open-live-two-nested.png)
- [09-main-live-output-click.png](evidence/agent-live-correction/09-main-live-output-click.png)

## subagents-background-09

Agent inspection tabs disappear on restart despite the Panel persistence contract

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A user returning to a long review loses the agent inspection tabs and can land on a surviving Background tab instead.

Recommendation. Persist stable agent identities and reconstruct saved drills lazily, or make this exception an explicit product decision with a clear recoverable list of recent agents. Preserve deliberate active-tab choice when an ephemeral tab cannot restore.

Acceptance check. Open Overview, Background and Agent A with A active, restart, and verify A restores with saved history or an explicit recovery state. The application does not silently show Background as if the user chose it.

[Full investigation and reproduction conditions](reports/subagents-background.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/panel.rs:124-140](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L124-L140)
- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:370-388](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L370-L388)
- [docs/adr/0039-the-panel-is-one-tabbed-surface-host.md:17-31](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/docs/adr/0039-the-panel-is-one-tabbed-surface-host.md#L17-L31)
- [apps/web/src/components/chat/useThreadLifecycle.tsx:466-495](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L466-L495)

## transcript-001

Find cannot discover text hidden by turn or tool disclosures

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A reader can receive zero matches for text that exists in the loaded conversation. The new default turn folding makes this common for intermediate explanations and command output.

Recommendation. Build a search index from semantic messages and tool details. Keep visible-content selection separate. Map each hit to the disclosure chain and stable content identity.

Acceptance check. A token present only in a collapsed intermediate answer or collapsed command result is counted before expansion. Next match opens the required disclosures and reveals the exact occurrence. Closing Find preserves the reader's place.

[Full investigation and reproduction conditions](reports/transcript.md).

Evidence:

- [crates/ferrite-pane/src/transcript/find.rs:3-9](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/find.rs#L3-L9)
- [crates/ferrite-pane/src/transcript/find.rs:99-111](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/find.rs#L99-L111)
- [crates/ferrite-pane/src/transcript.rs:1201-1247](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1201-L1247)
- [crates/ferrite-pane/src/transcript/rows.rs:549-559](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/rows.rs#L549-L559)
- [apps/web/src/components/chat/MessagesTimeline.tsx:639-662](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L639-L662)
- [apps/web/src/components/chat/MessagesTimeline.tsx:714-729](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L714-L729)
- [apps/web/src/components/chat/TimelineUserMessageRow.tsx:298-299](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L298-L299)

Review note. Find reported no matches for text inside a folded Read call; after disclosure it returned 1/1 for a unique final-passage phrase. Closed-turn search remains source-only. The separate screenshot-only paging blankness is inconclusive because headless repaint omitted unrelated shell regions.

Runtime evidence:

- [scroll-runtime-correction.md](reports/scroll-runtime-correction.md)
- [05-folded-find.png](evidence/scroll-correction/05-folded-find.png)
- [07-expanded-find-last.png](evidence/scroll-correction/07-expanded-find-last.png)

## transcript-002

Long transcript retention has no equivalent of earlier-turn paging

P2. missing-capability. inferred. Disposition `capability-candidate`.

Readers of long sessions cannot reliably return to the earliest prompts or search them from the transcript. Stored events may still exist; this is a navigation and retrieval limitation, not a claim of data loss.

Recommendation. Add a stored-history cursor and older-page loading to the main transcript. Preserve bounded memory with a sliding window. Show partial coverage rather than silently presenting the retained start as the conversation start.

Acceptance check. A fixture larger than both retention budgets can reach its original prompt through an earlier-history control. Prepending a page does not move the current visible sentence. A failed page load shows a retry action. Confirm the installed build before promoting this inferred gap to a reproduced defect.

[Full investigation and reproduction conditions](reports/transcript.md).

Evidence:

- [crates/ferrite-core/src/activity.rs:324-337](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/activity.rs#L324-L337)
- [crates/ferrite-core/src/activity.rs:756-815](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/activity.rs#L756-L815)
- [crates/ferrite-core/src/transcript.rs:2952-2958](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/transcript.rs#L2952-L2958)
- [crates/ferrite-core/src/cockpit/mirror.rs:313-324](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/cockpit/mirror.rs#L313-L324)
- [crates/ferrite-pane/src/transcript.rs:140-191](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L140-L191)
- [crates/ferrite-pane/src/transcript.rs:273-311](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L273-L311)
- [apps/web/src/components/chat/MessagesTimeline.tsx:1505-1534](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L1505-L1534)

## transcript-003

Successful turns erase the visible plan history

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

Readers cannot check which planned steps were completed or left pending after a successful answer. Expanding Worked does not recover the checklist.

Recommendation. Snapshot the last checklist state per turn and render it as historical work. Retain the useful blocked presentation for interrupted turns, with no live spinner after settlement.

Acceptance check. After a successful turn ends, expand Worked and inspect the original ordered steps and their final statuses. A pending step remains visibly pending. The same historical plan survives reopening the thread.

[Full investigation and reproduction conditions](reports/transcript.md).

Evidence:

- [crates/ferrite-pane/src/pane/working.rs:627-655](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/working.rs#L627-L655)
- [crates/ferrite-core/src/transcript.rs:331-335](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/transcript.rs#L331-L335)
- [crates/ferrite-pane/src/transcript/rows.rs:413-427](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/rows.rs#L413-L427)
- [crates/ferrite-pane/src/pane/working.rs:816-843](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/working.rs#L816-L843)
- [apps/web/src/components/chat/MessagesTimeline.logic.ts:1966](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.logic.ts#L1966)
- [apps/web/src/components/chat/MessagesTimeline.logic.ts:1992](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.logic.ts#L1992)
- [apps/web/src/components/chat/V2ItemInspector.tsx:404-412](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/V2ItemInspector.tsx#L404-L412)

## transcript-004

Opening one tool removes the entire work-group height cap

P2. interaction. source-proven. Disposition `repair-or-quality`.

Inspecting one result in a large group can expand all its compact rows into the main transcript and displace surrounding content far beyond the intended bounded work list.

Recommendation. Retain the base cap of min(288 px, half the viewport) and add measured detail height. Keep the group's scroll anchor when children open and close.

Acceptance check. With 100 compact calls, opening a two-line result grows the group only by that result's added height. The other compact calls remain internally scrollable. Closing the child restores the previous group position.

[Full investigation and reproduction conditions](reports/transcript.md).

Evidence:

- [crates/ferrite-pane/src/transcript.rs:1821-1848](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1821-L1848)
- [crates/ferrite-pane/src/pane.rs:4485-4488](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L4485-L4488)
- [crates/ferrite-pane/src/pane.rs:4507-4514](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L4507-L4514)
- [apps/web/src/components/chat/TimelineWorkGroupRows.tsx:231-241](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineWorkGroupRows.tsx#L231-L241)
- [apps/web/src/components/chat/TimelineWorkGroupRows.tsx:274-281](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineWorkGroupRows.tsx#L274-L281)

## transcript-006

Citations and copied text do not retain a navigable source message

P2. partial-capability. source-proven. Disposition `repair-or-quality`.

A cited passage reaches the model, but the reader cannot jump back to the original answer or copy a stable link to a specific prompt. Repeated text makes manual relocation ambiguous.

Recommendation. Add optional structured provenance to Reply without breaking plain-text provider prompts. Add a prompt/message deep link and a source-opening action on sent citation chips.

Acceptance check. A citation made from a folded historical response opens the exact original passage after reopening the thread. A copied prompt link lands on that prompt. Duplicate passages resolve by message identity, not substring alone.

[Full investigation and reproduction conditions](reports/transcript.md).

Evidence:

- [crates/ferrite-pane/src/quote.rs:50-85](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/quote.rs#L50-L85)
- [crates/ferrite-pane/src/transcript/reply.rs:206-220](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/reply.rs#L206-L220)
- [crates/ferrite-pane/src/transcript.rs:1565-1593](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1565-L1593)
- [crates/ferrite-pane/src/transcript.rs:303-305](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L303-L305)
- [apps/web/src/components/chat/TimelineAssistantRows.tsx:220-243](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineAssistantRows.tsx#L220-L243)
- [apps/web/src/components/chat/MessagesTimeline.tsx:664-706](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L664-L706)
- [apps/web/src/components/chat/TimelineUserMessageRow.tsx:834-840](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L834-L840)

## transcript-007

Markdown math is displayed as code instead of typeset formulas

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Technical explanations containing fractions, matrices or aligned equations remain raw TeX-like text and are harder to read.

Recommendation. Add an optional math rendering path with literal fallback and source-preserving copy. This is a capability choice beyond ADR 0038's narrower work-group contract, not evidence that the recent styling migration failed.

Acceptance check. Inline fractions and display matrices render correctly in both themes. Partial invalid streaming remains readable. Copy returns the source equation and wide formulas remain keyboard-scrollable.

[Full investigation and reproduction conditions](reports/transcript.md).

Evidence:

- [vendor/gpui-base/src/text/format/markdown.rs:233-237](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/format/markdown.rs#L233-L237)
- [vendor/gpui-base/src/text/format/markdown.rs:413-417](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/format/markdown.rs#L413-L417)
- [crates/ferrite-pane/src/rich.rs:647-655](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L647-L655)
- [apps/web/src/components/ChatMarkdown.tsx:1414-1421](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1414-L1421)
- [apps/web/src/components/chat/MarkdownMath.tsx:9-12](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownMath.tsx#L9-L12)
- [apps/web/src/components/chat/MarkdownMath.tsx:33-74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownMath.tsx#L33-L74)

## transcript-008

Transcript Markdown has no inline audio or video playback

P2. missing-capability. source-proven. Disposition `capability-candidate`.

Generated or referenced audio and video require leaving the transcript. Image-style media references cannot become the inline players T3 provides.

Recommendation. Classify media references before image rendering and provide accessible native playback or an intentional media attachment fallback. Treat this as optional parity investment according to the product's media workflows.

Acceptance check. An MP3 and an MP4 referenced in an assistant answer each expose playback controls or a labeled open action. Failed media does not appear as an unexplained broken image. Playback stops appropriately when its transcript is released.

[Full investigation and reproduction conditions](reports/transcript.md).

Evidence:

- [vendor/gpui-base/src/text/format/markdown.rs:225-231](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/vendor/gpui-base/src/text/format/markdown.rs#L225-L231)
- [crates/ferrite-pane/src/rich.rs:535-575](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L535-L575)
- [crates/ferrite-pane/src/rich.rs:647-655](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/rich.rs#L647-L655)
- [crates/ferrite-pane/src/pane.rs:3800-3832](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane.rs#L3800-L3832)
- [apps/web/src/components/ChatMarkdown.tsx:1490-1518](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1490-L1518)
- [apps/web/src/components/chat/ChatMarkdownMedia.tsx:298-321](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatMarkdownMedia.tsx#L298-L321)

## BROWSER-15

Errors occupy an inline text row with repetitive recovery copy

P3. interaction. source-proven. Disposition `repair-or-quality`.

Failure details compete with the page and can wrap through much of a narrow panel. The recovery action is described in text rather than placed beside the message.

Recommendation. Classify load, HTTP, engine, and profile errors. Give each a short summary and relevant action, with details behind disclosure. Keep layout stable and decide whether the failed page is meaningfully annotatable.

Acceptance check. Offline, refused connection, HTTP error, and unavailable engine each show one concise explanation and the correct retry/repair action at narrow panel widths, without repeated recovery sentences.

[Full investigation and reproduction conditions](reports/browser.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/browser.rs:1452-1453](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1452-L1453)
- [crates/ferrite-cockpit/src/cockpit/browser.rs:1533-1545](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1533-L1545)
- [crates/ferrite-pane/src/visual/web/cef/handlers.rs:628-638](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L628-L638)
- [apps/web/src/components/preview/PreviewUnreachable.tsx:54-106](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewUnreachable.tsx#L54-L106)

## composer-12

There is no prompt stash independent of the active draft

P3. missing-capability. source-proven. Disposition `capability-candidate`.

Users cannot temporarily set aside several prompts and their context while composing something else. Per-thread autosave and sent-prompt history do not provide this workflow.

Recommendation. Treat stash as an optional product addition after draft durability. Reuse structured draft snapshots and explicit deletion/retention rules.

Acceptance check. Stash two prompts including images and quotes, restart, compose a third, then restore either stash. No item sends implicitly and attachment expiry/failure is visible.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-core/src/drafts.rs:12-17](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/drafts.rs#L12-L17)
- [crates/ferrite-cockpit/src/cockpit.rs:6617-6650](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L6617-L6650)
- [crates/ferrite-pane/src/composer.rs:41-80](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L41-L80)
- [apps/web/src/components/chat/useComposerStash.ts:461-522](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerStash.ts#L461-L522)
- [apps/web/src/components/chat/ComposerStashMenu.tsx:1-150](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerStashMenu.tsx#L1-L150)

## composer-13

The native composer lacks rich Markdown and code-block editing

P3. intentional-difference. source-proven. Disposition `product-choice`.

Formatting is plain source text with token highlighting. Users do not get T3 list continuation, task nesting, code indentation, syntax highlighting or language selection while composing.

Recommendation. Keep plain text as a supported choice. If richer editing is desired, first define exact Markdown round-trip and shortcut behavior, especially Enter inside code.

Acceptance check. Lists nest/continue predictably, code blocks indent and choose language, and toggling rich/plain preserves the exact submitted Markdown. Test IME, Unicode, multiline selection and undo across formatting boundaries.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-pane/src/line.rs:1-32](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/line.rs#L1-L32)
- [crates/ferrite-pane/src/composer.rs:787-790](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L787-L790)
- [apps/web/src/components/ComposerPromptEditorTiptap.tsx:810-870](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L810-L870)
- [docs/user/composer.md:15-36](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L15-L36)

## composer-14

There is no integrated voice-input workflow

P3. missing-capability. source-proven. Disposition `capability-candidate`.

T3 users can dictate into a draft with recording, transcription and cancellation feedback. Ferrite has no corresponding composer control or state. OS-level dictation may still work and was not tested.

Recommendation. Treat this as a product capability choice. If added, reuse explicit draft ownership and cancellation rules; disclose the transcription destination.

Acceptance check. Start voice input, dictate, finish, and verify text lands at the saved selection without sending. Cancel and microphone failure preserve the original draft. Changing threads does not insert text into the wrong draft.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-pane/src/composer.rs:41-80](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/composer.rs#L41-L80)
- [crates/ferrite-cockpit/src/cockpit.rs:8291-8301](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L8291-L8301)
- [apps/web/src/components/chat/ComposerSpeechButton.tsx:16-39](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerSpeechButton.tsx#L16-L39)
- [docs/user/composer.md:199-203](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L199-L203)

## composer-15

A first prompt cannot fan out to multiple chosen models

P3. missing-capability. source-proven. Disposition `capability-candidate`.

A user comparing models must manually create and submit several threads. T3 has a single draft action that launches selected models in isolated worktrees.

Recommendation. Keep this optional and separate from ordinary send. If adopted, model a fan-out submission explicitly instead of overloading one thread binding.

Acceptance check. Send one draft to two models. Two isolated, clearly labeled threads receive identical prompt/context. Failure of one leaves its submission recoverable without resending the successful one.

[Full investigation and reproduction conditions](reports/composer.md).

Evidence:

- [crates/ferrite-core/src/draft.rs:27-35](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/draft.rs#L27-L35)
- [crates/ferrite-cockpit/src/cockpit.rs:7854-7877](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L7854-L7877)
- [apps/web/src/components/chat/composerSend.logic.ts:39-59](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/composerSend.logic.ts#L39-L59)
- [apps/web/src/components/chat/useComposerSend.ts:1110-1130](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1110-L1130)

## coverage-quality-06

Factory schedules do not offer one-time or sub-hour scheduled prompts

P3. partial-capability. source-proven. Disposition `repair-or-quality`.

Optional T3 parity gap for lightweight reminders or frequent prompts. Ferrite already supports recurring Factory automation and should not be described as lacking scheduling.

Recommendation. Make an explicit product decision about lightweight thread scheduling. If adopted, add one-time scheduling and destination selection while preserving missed/overlap policies.

Acceptance check. An adopted one-time task runs once after restart, shows its destination and result, and does not silently repeat or duplicate an active run.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [crates/ferrite-core/src/factory/starts.rs:80-153](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/factory/starts.rs#L80-L153)
- [crates/ferrite-core/src/api/schema.rs:204-213](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/api/schema.rs#L204-L213)
- [packages/contracts/src/scheduledTask.ts:25-74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/scheduledTask.ts#L25-L74)
- [packages/contracts/src/scheduledTask.ts:202-215](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/scheduledTask.ts#L202-L215)
- [apps/web/src/components/settings/ScheduledTasksSettings.tsx:1250-1283](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ScheduledTasksSettings.tsx#L1250-L1283)

## coverage-quality-07

No first-class guest thread participation was found

P3. missing-capability. inferred. Disposition `capability-candidate`.

Optional collaboration gap. No discovered Ferrite entry point lets another person view one thread and propose owner-approved messages.

Recommendation. Prioritize after broken primary workflows. If collaboration is adopted, define read scope, expiry, revocation and owner approval before exposing a share control.

Acceptance check. If adopted, the guest sees only the intended thread, proposed messages await owner action, and revocation ends access. Otherwise document the deliberate product boundary.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [crates/ferrite-core/src/api/schema.rs:118-137](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/api/schema.rs#L118-L137)
- [crates/ferrite-core/src/api/schema.rs:244-257](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/api/schema.rs#L244-L257)
- [crates/ferrite-cockpit/src/palette.rs:139-190](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/palette.rs#L139-L190)
- [crates/ferrite-ui/src/frame/settings.rs:15-43](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/frame/settings.rs#L15-L43)
- [packages/contracts/src/guestShare.ts:13-70](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/guestShare.ts#L13-L70)
- [apps/web/src/components/chat/GuestShareControls.tsx:73-202](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/GuestShareControls.tsx#L73-L202)

Review note. The absence claim remains an inference from bounded repository searches, not proof that every possible external collaboration route is impossible.

## coverage-quality-08

T3 device panels have no Ferrite panel counterpart

P3. missing-capability. source-proven. Disposition `capability-candidate`.

Optional mobile-development parity gap. Ferrite's panel model has no embedded iOS or Android device tab; external provider device tools remain possible.

Recommendation. Treat as a product boundary until current panel/browser interactions are reliable. If adopted, use the existing tab identity and focus model.

Acceptance check. An adopted device opens one identified tab, repeated opens activate it, device input remains scoped, and disconnect/reconnect is visible without breaking other panel focus.

[Full investigation and reproduction conditions](reports/coverage-quality.md).

Evidence:

- [crates/ferrite-cockpit/src/cockpit/surfaces.rs:25-46](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/surfaces.rs#L25-L46)
- [apps/web/src/rightPanelStore.ts:30-40](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L30-L40)
- [apps/web/src/components/chat/RightPanelSurfaceContent.tsx:187-201](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L187-L201)

## motion-004

Popover and menu entrances are globally disabled

P3. motion. source-proven. Disposition `repair-or-quality`.

Model and other popovers, context menus, slash commands and mention menus appear and disappear instantly.

Recommendation. Separate menus and popovers in the policy, add a short origin-aware entrance and retained noninteractive exit where requested, and preserve immediate keyboard selection. A global MENU_IN duration change alone does not implement exits.

Acceptance check. Open and close model, attachment, slash, mention, project-filter and context menus at every window edge. Verify the entrance originates at the anchor, closing immediately releases input, and rapid reopen does not show duplicate faces.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-ui/src/theme.rs:3662](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/theme.rs#L3662)
- [crates/ferrite-ui/src/motion.rs:473-489](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L473-L489)
- [crates/ferrite-cockpit/src/cockpit/context_menu.rs:1001](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/context_menu.rs#L1001)
- [crates/ferrite-cockpit/src/cockpit.rs:3333-3336](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit.rs#L3333-L3336)
- [crates/ferrite-pane/src/transcript.rs:2392-2395](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L2392-L2395)
- [apps/web/src/components/ui/popover.tsx:95](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/popover.tsx#L95)
- [apps/web/src/components/ui/popover.tsx:114](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/popover.tsx#L114)
- [apps/web/src/components/ui/menu.tsx:56](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/menu.tsx#L56)
- [apps/web/src/components/ui/combobox.tsx:170](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/combobox.tsx#L170)

## motion-009

Long programmatic scrolls can snap before their final ease

P3. partial-capability. source-proven. Disposition `repair-or-quality`.

A distant jump to the latest content can teleport across most history and animate only the last viewport. A send target that has no measured bounds can land immediately.

Recommendation. Decide and document a long-distance policy. If full spatial continuity is required, use measured estimates or staged movement that respects virtualization. If a shortened jump is desired, make that intentional and verify it visually instead of describing every jump as smooth.

Acceptance check. From several screens above the tail, activate Scroll to end and record the first frame through completion. Repeat a send with an unmeasured target. The observed path must match the chosen policy and remain interruptible.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-pane/src/transcript/scroll.rs:159-203](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/scroll.rs#L159-L203)
- [apps/web/src/components/chat/useTimelineFollow.ts:408](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L408)

## motion-011

Several transient elements animate in but disappear immediately

P3. motion. source-proven. Disposition `repair-or-quality`.

Find bars, notices and background-work lines have asymmetric lifecycle feedback; layout can change abruptly on dismissal or completion.

Recommendation. Use a small retained exit where it helps the user track completion or dismissal. Release hit testing immediately, and collapse occupied space in the same transition. Keep high-frequency text updates and direct manipulation immediate.

Acceptance check. Dismiss find, resolve a notice, finish background work, and remove an attachment while the composer is populated. Verify stable focus and intentional exit behavior without duplicate controls or hidden clickable remnants.

[Full investigation and reproduction conditions](reports/motion.md).

Evidence:

- [crates/ferrite-ui/src/motion.rs:31-32](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L31-L32)
- [crates/ferrite-ui/src/motion.rs:48-51](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/motion.rs#L48-L51)
- [crates/ferrite-pane/src/find.rs:322-325](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/find.rs#L322-L325)
- [crates/ferrite-pane/src/pane/background_work.rs:167-170](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/background_work.rs#L167-L170)
- [crates/ferrite-pane/src/pane/composer.rs:548-558](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/pane/composer.rs#L548-L558)
- [apps/web/src/components/chat/ThreadNow.tsx:24](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadNow.tsx#L24)

## pr-operations-005

Timeline reverses the backend order and has no ordering control

P3. defect. source-proven. Disposition `repair-or-quality`.

New activity appears at the bottom of a long discussion despite the panel documenting newest-first behavior.

Recommendation. Remove the accidental reversal or define a shared order contract and add a user preference.

Acceptance check. With comments at 09:00 and 10:00, the default Timeline begins with 10:00. Any oldest-first option visibly changes the order.

[Full investigation and reproduction conditions](reports/pr-operations.md).

Evidence:

- [crates/ferrite-reviewr/src/forge.rs:1025-1028](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-reviewr/src/forge.rs#L1025-L1028)
- [crates/ferrite-core/src/protocol/review.rs:652-678](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/protocol/review.rs#L652-L678)
- [crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:789-795](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs#L789-L795)
- [apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx:753-797](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestTimelineTab.tsx#L753-L797)

## PS-09

Appearance is deliberately dark-only with fixed font families

P3. intentional-difference. source-proven. Disposition `product-choice`.

Users cannot choose a light/system theme, import a theme, or adjust interface and monospace font families independently. Transcript size and notification customization do exist.

Recommendation. Keep the distinction visible in the parity inventory. Revisit dark-only and typography scope based on user preference rather than copying T3's whole theme editor.

Acceptance check. If expanded, change appearance and fonts, restart and inspect transcript, composer, menus, terminal and settings at representative sizes. If scope stays fixed, documentation accurately states the supported choices.

[Full investigation and reproduction conditions](reports/providers-settings.md).

Evidence:

- [crates/ferrite-ui/src/theme.rs:172-173](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/theme.rs#L172-L173)
- [crates/ferrite-ui/src/theme.rs:1263-1276](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-ui/src/theme.rs#L1263-L1276)
- [crates/ferrite-core/src/settings.rs:28-147](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/settings.rs#L28-L147)
- [crates/ferrite-core/src/settings.rs:188-205](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-core/src/settings.rs#L188-L205)
- [apps/web/src/components/settings/AppearanceSettingsPanel.tsx:87](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AppearanceSettingsPanel.tsx#L87)
- [apps/web/src/components/settings/AppearanceSettingsPanel.tsx:145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AppearanceSettingsPanel.tsx#L145)
- [apps/web/src/components/settings/TypographySettings.tsx:74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/TypographySettings.tsx#L74)
- [apps/web/src/components/settings/TypographySettings.tsx:144](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/TypographySettings.tsx#L144)

## transcript-005

Pin as chapter is a transient label without chapter navigation

P3. partial-capability. source-proven. Disposition `repair-or-quality`.

The menu promises a way to mark a useful answer, but there is no chapter list to retrieve it and a new transcript view starts with no pins.

Recommendation. Either complete persistent chapter navigation using stable message identities or remove this incomplete menu action until its purpose is supported. This is Ferrite's own incomplete feature, not a requirement inferred from T3.

Acceptance check. Pin an answer, restart, open the same thread and use a chapter list to jump to it. If the feature is intentionally temporary, the action name and feedback explicitly say so.

[Full investigation and reproduction conditions](reports/transcript.md).

Evidence:

- [crates/ferrite-pane/src/transcript/message_menu.rs:19-20](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L19-L20)
- [crates/ferrite-pane/src/transcript/message_menu.rs:173-177](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript/message_menu.rs#L173-L177)
- [crates/ferrite-pane/src/transcript.rs:483-484](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L483-L484)
- [crates/ferrite-pane/src/transcript.rs:611](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L611)
- [crates/ferrite-pane/src/transcript.rs:1497-1503](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/transcript.rs#L1497-L1503)
