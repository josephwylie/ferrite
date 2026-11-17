# Thread context card

The target is the context card in T3 commit `4da2bc520c38cd6b36014d2888774168fbf4c77f`. This record describes the implemented card unit and its remaining operation gaps. It does not claim complete product equivalence.

## State and presentation

`ThreadSurfaces.details_inline_open` owns the saved per-thread visibility. Older `panel.json` records default to visible. `DetailsCard` owns transient popover visibility, focus, scrolling, and measurements. Entering inline mode clears popover visibility without changing the saved inline choice.

The card is 280 px wide with a 12 px outer gap. Inline placement requires 160 px usable height and preserves the 640 px minimum conversation lane, 48 px lane padding, and 32 px card clearance. The fit check uses the smallest current, starting, and target pane widths during column transitions. The transcript body's bounds supply the available height, excluding the Composer and Pane header. Its bounds observer requests another frame only when those bounds change. The find bar adds its top inset. Ferrite's preview owner is a docked Panel Surface, so its width is already part of the target pane geometry. Ferrite has no separate floating preview frame to exclude.

Full and compact measurements have independent keys and heights. Keys contain the actual resolved rows, their values, the thread, and the card width. A stale layout callback cannot restore an earlier measurement. PR, lineage, and side-chat list rows do not consume the density fit budget. These rows remain in the card's scroll area.

The card now has its own composition in `cockpit/details/content.rs`. Overview no longer contains unused card-density branches.

Sources are `apps/web/src/components/chat/threadDetailsCardLayout.ts`, `ThreadDetailsCard.tsx`, `ThreadDetailsPanel.tsx`, and `apps/web/src/rightPanelStore.ts` in the pinned checkout at `/tmp/ferrite-t3-reference`.

## Item preferences

All twelve stable IDs, labels, and defaults come from `threadDetailsItems.ts`. Usage limits and Context window default off. The other ten items default on. Settings stores only choices that differ from these defaults.

The card's right-click menu exposes every item and Reset to default. An empty card exposes Choose what to show. The menu uses Ferrite's existing keyboard-aware context-menu owner. Preferences save through `change_settings`.

| Item | Allowed density | Implemented data and action |
| --- | --- | --- |
| Environment and worktree | Full | Existing machine, checkout, and worktree facts. The folder opens through the native file owner. |
| Open in editor | Full and compact | Existing native file-manager option for a local folder. T3 includes `file-manager` in `OpenInPicker.tsx`. |
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

## Remaining operation gaps

These are distinct from an empty-data state and remain open for the overall parity review.

- Environment and branch rows do not provide T3's environment, worktree, and branch selectors.
- The native file-manager option does not provide installed-editor discovery, the editor picker, or a saved editor preference.
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

Tests cover saved inline visibility, older panel records, independent density invalidation, stale measurement callbacks, the density matrix, actual row-value keys, item defaults and override storage, pane recreation, empty-card recovery, and preference reset.
