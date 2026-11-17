# Thread context card

The target is the context card in T3 commit `4da2bc520c38cd6b36014d2888774168fbf4c77f`. This record describes the implemented card unit and its remaining operation gaps. It does not claim complete product equivalence.

## State and presentation

`ThreadSurfaces.details_inline_open` owns the saved per-thread visibility. Older `panel.json` records default to visible. `DetailsCard` owns transient popover visibility, focus, scrolling, and measurements. Entering inline mode clears popover visibility without changing the saved inline choice.

The card is 280 px wide with a 12 px outer gap. Inline placement requires 160 px usable height and preserves the 640 px minimum conversation lane, 48 px lane padding, and 32 px card clearance. The fit check uses the smallest current, starting, and target pane widths during column transitions. The transcript body's bounds supply the available height, excluding the Composer and Pane header. Its bounds observer requests another frame only when those bounds change. The find bar adds its top inset. Ferrite's preview owner is a docked Panel Surface, so its width is already part of the target pane geometry. Ferrite has no separate floating preview frame to exclude.

Full and compact measurements have independent keys and heights. Keys contain the actual resolved rows, their values, the thread, and the card width. A stale layout callback cannot restore an earlier measurement. PR list rows and the entire lineage and side-chat sections do not consume the density fit budget. For those sections this includes the heading, 4 px heading gap, 18 px vertical padding, and any divider. They remain in the card's scroll area, matching the source DOM bounds excluded by `ThreadDetailsCard.tsx:84`.

The card now has its own composition in `cockpit/details/content.rs`. Overview no longer contains unused card-density branches.

Popover placement uses the existing anchored/deferred mechanism at the measured chat-header edge. T3's `ChatView.tsx:3351` supplies the full header as the anchor. The adjoining transcript canvas supplies that edge in Ferrite. The popup has 8 px padding and a 4 px alignment offset, leaving the card 12 px from the right chat edge and 8 px below the header. Its density budget is `canvas.height - 52`; its rendered height also respects the source's viewport and available-height limits. Inline placement retains the 12 px top gap. The measured content cannot shrink inside the scroll port, so overflowing full content causes a real density change.

Sources are `apps/web/src/components/chat/threadDetailsCardLayout.ts`, `ThreadDetailsCard.tsx`, `ThreadDetailsPanel.tsx`, and `apps/web/src/rightPanelStore.ts` in the pinned checkout at `/tmp/ferrite-t3-reference`.

## Density comparison

Density depends on the measured, non-scrolling content height, not fixed window breakpoints. Both implementations first try Full when its measurement is unknown or fits. They then try Compact when its measurement is unknown or fits, and finally Essential. Equality counts as fitting. A capture with only a few rows cannot establish the other two densities.

| Density | Source presentation | Ferrite presentation |
| --- | --- | --- |
| Full | Applicable environment, editor, scripts, branch/PR, Git, changes, servers, automation, relationships, and enabled usage/context content. | Implemented rows follow their preferences; operation gaps below still remove source content. |
| Compact | Hides environment, servers, automation, lineage, and side chats. Keeps editor. Removes the workspace/version-control divider. Idle Git actions disappear; progress/success can remain. | The same visibility rules apply to implemented content. There is no direct Git progress/success row to prove against the source. |
| Essential | Compact rules, additionally hiding the editor. Scripts, branch/PR, changes, enabled usage/context, and applicable Git progress can remain. | The editor disappears; implemented branch/PR, changes, and enabled usage/context remain. |

The allowed-item matrix test is supplemented by measurement-transition, stale-key, and excluded-list-bounds regressions. A GPUI resize regression feeds both usage windows and a known context maximum through the existing session events, preserves the content and preferences, and shrinks the actual canvas through `details-density-full`, `details-density-compact`, and `details-density-essential`. It checks that environment disappears first, editor disappears next, and both usage rows and context survive. A separate geometry assertion checks the popup against the measured header/chat edge. Parent-owned native comparison remains necessary. A different set of source operations changes the measured fit thresholds, so matching density machinery does not establish full card parity.

## Item preferences

All twelve stable IDs, labels, and defaults come from `threadDetailsItems.ts`. Usage limits and Context window default off. The other ten items default on. Settings stores only choices that differ from these defaults.

The card's right-click menu exposes every item and Reset to default. An empty card exposes Choose what to show. The menu uses Ferrite's existing keyboard-aware context-menu owner. Preferences save through `change_settings`.

| Item | Allowed density | Implemented data and action |
| --- | --- | --- |
| Environment and worktree | Full | Existing machine, checkout, and worktree facts. The folder opens through the native file owner. |
| Open in editor | Full and compact | A split launch button and editor picker for a local folder. The picker lists installed editors in T3's order and saves the preferred editor. The primary button launches it; Finder uses the existing native file owner. |
| Project scripts | All | The preference exists. No project-script projection supplies rows. Global user commands are not substituted. |
| Branch and pull request | All | Existing branch facts and PR row. The PR opens its existing Panel Surface. |
| Git actions | All, with idle actions omitted outside full | Existing Create PR operation when the branch has dirty files and no PR. This asks the agent to commit, push, and create the PR. |
| Changes | All | Existing Git status supplies the dirty-file count. The row opens the existing Changes Surface where that opener is available. |
| Servers | Full | The preference exists. No discovered-port projection supplies rows. MCP connectors are not substituted. |
| Automations | Full | The preference exists. No thread-bound automation projection supplies rows. Factory workflow schedules are not substituted. |
| Lineage | Full | Existing activity children and their status. Each row opens the existing Agent Surface. |
| Side chats | Full | Existing side-chat Surface IDs, titles, and running status. Each row activates that chat. |
| Usage limits | All | Existing provider account windows. Bars and percentages show the room left. |
| Context window | All | Existing token occupancy and optional context-window maximum. The bar shows usage. |

`ThreadDetailsPanel.tsx:169` defines the density matrix. `ThreadServersPanel.tsx:62` hides the server section when no discovered server exists. `ThreadAutomationsPanel.tsx:84` hides an empty, successful query. `ThreadRelationshipsControl.tsx:621` defines the lineage and side-chat presence rules.

## Editor picker

`details/editors.rs` owns the installed choices, discovery state, effective preference, and launch errors. `Settings.preferred_editor` stores the stable string ID. An unavailable saved editor falls back to the first installed editor in the source contract's order. Choosing a dropdown row only changes the preference. The primary action opens the current thread's folder with that editor.

Discovery runs once per Cockpit off the UI thread and refreshes when the picker opens. Rendering reads the cached result. It checks executable PATH entries, macOS application bundles and versioned JetBrains bundles, Toolbox scripts, and the source's Windows and Linux install locations. Cursor receives `--classic`; Kiro's CLI receives `ide`, while its application launcher does not. Zed's Linux/OpenZFS command collision is checked with a bounded version probe. Windows PATH candidates prefer stable Zed over Preview/Nightly. Editor launches use argv, preserve paths containing spaces and shell characters, remove `ELECTRON_RUN_AS_NODE`, and report spawn errors through existing notifications. The existing native FileLink owner opens the file manager.

The primary row is 32 px high, with a separate 32 px dropdown. Its menu matches the row's width and opens 4 px below it through the existing keyboard-aware menu owner. Source SVGs supply full-color editor marks in both the button and menu. They are adapted from `Icons.tsx` and `JetBrainsIcons.tsx`, embedded through `ferrite-ui::icons`, and retain T3's MIT notice in `assets/icons/editor-icons.LICENSE`.

The behavioral sources are `apps/web/src/components/chat/OpenInPicker.tsx`, `apps/web/src/editorPreferences.ts`, `packages/contracts/src/editor.ts`, `packages/shared/src/editor.ts`, and `apps/server/src/process/externalLauncher.ts` at the pinned commit. Ferrite adopts the login shell's environment at startup in `crates/ferrite/src/lib.rs`, before this discovery reads PATH.

The recorded native T3 picker in `evidence/t3-context-editor-picker-native.png` contains only Finder. A read-only check of this machine's PATH, application bundles, versioned bundle aliases, and JetBrains Toolbox launcher paths found no supported editor executable. Ferrite's Finder-only result is consistent with that evidence. Local server connections remain eligible: `cockpit/remote.rs::Remote::bridge` returns `None` for `Endpoint::Local`, so `CockpitView::on_machine` is false for the local server-backed GUI and true for remote machine bridges. No discovery guard was changed on the basis of the Finder-only result.

## Remaining operation gaps

These are distinct from an empty-data state and remain open for the overall parity review.

- Unlocked environment/worktree and branch selectors are not integrated into this card. T3's `BranchToolbar.tsx:670` requires multiple environments and a change handler for environment selection; started threads have no handler and retain an indicator. `BranchToolbarEnvironmentSelector.tsx:70` likewise renders a locked row without change/add capabilities. Ferrite's existing machine operation switches the whole Cockpit, while `engine::open_choice` applies branch choices before a new thread starts. Neither operation is substituted for changing a live thread's workspace.
- The editor picker is local. T3's remote SSH editor URLs and remote-host editor discovery are not connected here.
- T3 binds Open in preferred editor to Cmd+O. Ferrite's existing Cmd+O Reopen thread binding is preserved, and the editor menu does not display an unbound shortcut hint.
- The registry has no project-script records or script CRUD. Existing `user_commands` are global operator commands.
- Agent-backed Create PR does not provide T3's direct Commit, Push, Pull, initialization, publication, progress, or action-menu behavior.
- The card has no discovered local listening-server source with PID, port, stop, and pagination operations.
- Factory owns schedules and enablement, but the card has no T3-equivalent thread-bound task association or edit, run-now, and pause controls.
- Activity-child navigation covers existing agent history. It does not prove fork-parent, remote delegation, merge-back, detach, or stop parity.
- The card does not expose its preference menu through Appearance Settings.
- Usage rows do not yet include T3's reset-time and pace tooltips. Context rows do not yet render an auto-compaction threshold tick.

## Verification status

This worker ran no builds, tests, or GUI sessions under the parent's disk and serialization constraint. Rustfmt parsed the owned Rust files, and `git diff --check` passed. The parent owns test and runtime acceptance.

The narrow test commands for this unit are:

```sh
cargo nextest run -p ferrite-pane -E 'test(details::tests)'
cargo nextest run -p ferrite-core -E 'test(settings::tests)'
cargo nextest run -p ferrite-cockpit -E 'test(details) or test(surfaces::tests)'
```

Tests cover saved inline visibility, older panel records, independent density invalidation, stale measurement callbacks, the density matrix, actual row-value keys, item defaults and override storage, pane recreation, empty-card recovery, and preference reset. Editor regressions cover installed-choice fallback/order, versioned bundle and stable-channel selection, executable permission checks, literal launch arguments, missing executables, and saved menu choices without launching. The 22 adapted SVGs were XML-parsed during extraction. Runtime rendering and native launch acceptance remain with the parent.
