# T3 live conversation checklist

## Overview

Exercise complete T3 conversation workflows with GPT-6.1 Sol and the authorized Claude Sonnet variant. Match every observed behavior to the source that owns it. This checklist does not claim any live pass. Its 66 cases have 132 separate provider result slots, all initially `not-run`.

The pinned source is `/tmp/ferrite-t3-audit-source-20261113` at `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. Record the installed T3 build first. A version mismatch must be resolved before attributing runtime behavior to this revision. The parent owns native UI operation, model tasks, fixtures, evidence and cleanup. This worker started no app, model task or agent.

The machine-readable companion is [t3-live-conversation-checklist.json](t3-live-conversation-checklist.json). The existing Ferrite journey IDs below link this verification to the correction matrix; they do not imply that T3 and Ferrite share an implementation.

## Key concepts

- A complete observation includes the entry action, visible result, source match and return or recovery path. A widget, capability flag, helper test or other-provider pass is insufficient.
- Record `passed-bounded`, `failed-reproduced`, `inconclusive`, `not-available-with-observed-reason` or `not-reached` for the exact exercised branch. Split a case with mixed outcomes. Do not turn untested branches into passes.
- Static labels below come from source. `${...}` marks a dynamic label template. Record the installed value and customized keymap. Approval options can be provider-specific.
- Three task bundles are an execution structure, not a claim of only three model calls. Count every actual submit, queued delivery, steer and descendant. Use only the authorized Sol and Sonnet variants.
- Keep before/intermediate/after native evidence. A blank partial repaint that also loses unrelated chrome is inconclusive until a separate native observation confirms the product state.

The applied pstack principles changed the checklist in concrete ways. Experience First made the unit an action and return path. Prove It Works keeps every unexecuted slot `not-run`. Sequence Work into Verifiable Units groups the work into reusable live states while preserving per-case assertions.

`throughput checkpoint: n/a, read-only investigation`

## How it works

### Prepare and record the owned scratch workspace

Record the starting user surface, project/checkout, model, effort, permissions, panel dimensions, clipboard if practical, and custom keybindings. Give every audit root/fork/side/child a recognizable title and retain its ID. Keep a live ledger of started, completed and stopped work.

| Fixture | Required content |
| --- | --- |
| `audit-notes.md` | Harmless Markdown with FILE_MARKER_T3, TOOL_FIND_SENTINEL, a table/headings and at least 120 numbered lines; include no secrets or instructions to act on unrelated work. |
| `audit-code.py` | Small readable function with a known source-line target and no external side effects. |
| `audit-image.png` | Small image with a known visible marker. Parent creates and inspects it before the task; record its dimensions. |
| `audit-extra-1.png through audit-extra-6.png` | Tiny distinct scratch images for image-strip overflow and previous/next preview. |
| `audit-write.txt` | Initially absent. Only the deliberately approved bundle B action creates or changes it. |

### Run three linked bundles

#### A. Read, render, cite and control a running turn

Run Sol first, then the Sonnet lane. Match actual provider-specific behavior to the source references below.

Use this bounded prompt, substituting the actual scratch path only if needed.

```text
Work only in this disposable audit scratch project. Do not spawn subagents or access unrelated files. Read audit-notes.md and audit-code.py, and inspect the attached audit image. First give an interim report with STREAM_START_T3, the FILE_MARKER_T3 value, the visible image marker, 24 numbered observations, a small table, a Python code fence with a long line, a literal blockquote, a relative link to audit-code.py and a harmless https://example.com link. Then run exactly one foreground local command that emits TICK_T3_01 through TICK_T3_36, one every five seconds, with an absolute 180-second bound. Do not detach, daemonize or restart it after interruption unless explicitly instructed. This command is for observing output and turn controls. After it ends, give a short final report ending FINAL_T3_A. Do not change files, use network tools or launch background processes.
```

- Run the pre-send intake cases first. Use the bounded command window for live/scroll and queue actions; remove unused queue entries before automatic continuation.
- All UI case checks need not fit one 180-second window. If a state expires, mark the branch not-reached or deliberately request one separately recorded bounded continuation. Never leave the task running indefinitely to complete a checklist.
- Queue and citation acknowledgements, if sent, are additional actual turns. Keep them short and record them; three bundles do not mean only three provider calls.
- Repeat the provider-dependent send, stream, queue, steer and stop paths on Sonnet. Shared UI results still require evidence in each claimed lane.

**Suggested case order:** T3-C01, T3-C02, T3-C03, T3-C04, T3-C06, T3-C07, T3-C08, T3-C09, T3-C10, T3-C11, T3-C12, T3-C17, T3-C18, T3-C31, T3-C32, T3-C33, T3-C34, T3-C35, T3-C36, T3-C37, T3-C38, T3-C13, T3-C14, T3-C15, T3-C16, T3-C19, T3-C20, T3-C21, T3-C22, T3-C23, T3-C24, T3-C25, T3-C26, T3-C27, T3-C28, T3-C30, T3-C05.

#### B. Exercise decisions, native child work and lineage

Run Sol first, then the Sonnet lane. Match actual provider-specific behavior to the source references below.

Use this bounded prompt, substituting the actual scratch path only if needed.

```text
Work only in this disposable audit scratch project using the selected authorized model. Use your real structured question tool, if available, for two questions: choose ALPHA or BETA, and provide a short free-text marker. Do not imitate a structured question with prose; say plainly if the tool is unavailable. Then request the harmless creation of audit-write.txt containing the chosen markers, respecting any approval or denial. For a second approval trial, request a harmless scratch-only printf and do not retry it if denied. Create at most two descendants in total: one child named T3-AUDIT-CHILD to read and summarize audit-code.py, and, only if supported, one grandchild named T3-AUDIT-GRANDCHILD to read audit-notes.md. Use the same authorized model for descendants where configurable and report actual model choices. No further descendants. Keep each summary under 150 words. A child may run one foreground tick command bounded to 90 seconds for observing its live view; never detach it. Report the real child identifiers. Finish only when all owned work is completed or explicitly stopped.
```

- Set Supervised before starting the native session. A label changed mid-session does not prove effective Claude permission changes.
- Model compliance is not guaranteed. Record absent question/approval/child states honestly and distinguish native capabilities from application projections.
- If the selected provider cannot honor the child model restriction, record the observed model or avoid launching that child. Do not silently substitute a more expensive model.
- Keep a descendant ledger and use real Stop actions before independent liveness cleanup.

**Suggested case order:** T3-C39, T3-C40, T3-C41, T3-C42, T3-C43, T3-C44, T3-C45, T3-C46, T3-C47, T3-C48, T3-C49, T3-C50, T3-C51, T3-C52.

#### C. Use completed histories to verify recovery and return paths

Run Sol first, then the Sonnet lane. Match actual provider-specific behavior to the source references below.

Use this bounded prompt, substituting the actual scratch path only if needed.

```text
This is a bounded audit recovery check on the selected fork or side chat. In fewer than 80 words, acknowledge the supplied source marker and identify which conversation context you received. Do not use tools, edit files, create agents or start background work. End with RECOVERY_T3_C.
```

- Reuse bundle A/B histories; no new long task is required. At most one short fork reply and one short side-chat reply per provider are planned, with any extra acknowledgement or fresh-session turn counted separately.
- Rewind only a disposable owned fork. File rollback must be restricted to known scratch checkpoints.
- Normal app restart belongs to the parent after all owned work is settled and unrelated work is safe. Do not use abrupt termination as a substitute for normal restore.
- Close/reopen applies to surviving side-chat surfaces. It is not a claim that deleted arbitrary root threads can be reopened.

**Suggested case order:** T3-C53, T3-C54, T3-C55, T3-C56, T3-C57, T3-C58, T3-C59, T3-C60, T3-C61, T3-C29, T3-C62, T3-C63, T3-C64, T3-C65, T3-C66.

### Execute and record each case

The order above prioritizes transient live states. The numbered catalog below is the complete scope for this worker. Each case starts `not-run` for both providers; update the companion result slots with evidence after actual execution.

#### T3-C01 · Identify the actual build, provider, model, effort and permissions

**State:** phase 0; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** SETTINGS-01, PERM-01.

**Entry labels and routes:** `Provider and model trigger has a dynamic accessible name; record the installed label`; `Permissions: Supervised`; `Permissions: Auto-accept edits`; `Permissions: Auto`; `Permissions: Full access`; `Supervised`; `Ask before commands and file changes.`; `Auto-accept edits`; `Auto`; `Full access`; `Source defaults only; inspect installed custom keymap`; `Cmd+F`; `Cmd+S`; `Cmd+Shift+T`; `Cmd+W`; `Cmd+Shift+Enter`; `Alt+ArrowUp`; `Cmd+Enter`; `Effort`; `Options`; `The provider descriptor supplies the actual reasoning-level labels`.

1. Record installed T3 build/version and compare it with the pinned commit. Record the active project, checkout, account/provider, exact model display name, effort and permission label.
2. Open and dismiss the model, effort and permissions menus with mouse and keyboard; inspect any customized shortcuts before relying on defaults.

**Required visible outcome:** Sol and Sonnet are the explicitly authorized models, with no silent fallback. Selection persists into the task and the visible thread identifies the same provider. A disabled or missing authorized model has a recorded reason.

**Return and cleanup:** Save the original configuration and window/panel sizes for restoration; do not alter unrelated threads.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/ProviderModelPicker.tsx::ProviderModelPicker`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderModelPicker.tsx#L89); [`apps/web/src/components/chat/ComposerModelEffort.tsx::PermissionsMenu`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerModelEffort.tsx#L618); [`apps/web/src/components/chat/runtimeModeConfig.ts::runtimeModeConfig`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/runtimeModeConfig.ts#L14); [`packages/shared/src/keybindings.ts::DEFAULT_KEYBINDINGS`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L57); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/web/src/components/chat/ComposerModelEffort.tsx::ComposerEffortColumn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerModelEffort.tsx#L493).

#### T3-C02 · Retain a never-sent draft across navigation

**State:** phase 0; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-02, NAV-07.

**Entry labels and routes:** `Composer accessible name is passed by the owner; record the actual name`; `Provider and model trigger has a dynamic accessible name; record the installed label`; `Attach files`.

1. Create a named audit draft. Enter DRAFT_UNSENT_T3 and a multiline sentence, then add one fixture attachment.
2. Navigate to another owned thread and back before first send. Inspect project, checkout, model and attachment state.

**Required visible outcome:** The original editable text, attachment identity and thread context survive. No model turn starts and no different draft receives this content.

**Return and cleanup:** Keep this as the bundle A draft or remove only its fixture attachment and text after recording.

**Sol:** Verify editable draft/history restoration using real navigation or a normal restart after work is quiescent. Codex native snapshot support is not proof of UI restore; live-only pending callbacks do not survive restart.

**Sonnet:** Verify the actual transcript/side-chat and editable draft after restore. Claude does not declare native snapshots or restart-surviving pending requests; record clean recovery or explicit unavailable state, not invented continuity.

**Owning source:** [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/web/src/components/chat/ProviderModelPicker.tsx::ProviderModelPicker`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderModelPicker.tsx#L89); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L702); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197); [`apps/web/src/composerDraftStore.ts::composerDraftStore`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2673); [`apps/web/src/composerDraftStore.ts::partializeComposerDraftStoreState`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2246).

#### T3-C03 · Use ordinary rich-text editing without accidental send

**State:** phase 0; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** SHELL-03, COMPOSE-01, QUOTE-03.

**Entry labels and routes:** `Composer accessible name is passed by the owner; record the actual name`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`.

1. Enter prose, a list, a fenced code block, Unicode and a literal leading > line. Use mouse selection, arrow keys, Shift+Enter, Backspace, undo and redo.
2. Repeat a caret edit while a right panel is open and close a suggestion popup with Escape.

**Required visible outcome:** Text edits occur at the clicked caret, newline and undo behavior preserve content, and Escape dismisses the foreground layer. No send occurs before the intended submit.

**Return and cleanup:** Retain the final intended draft or clear the scratch edits without touching another draft.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C04 · Use mentions and slash commands through keyboard selection

**State:** phase 0; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** SHELL-03, COMPOSE-01, FILES-05.

**Entry labels and routes:** `Composer accessible name is passed by the owner; record the actual name`; `/model`; `/side`; `/btw`; `/clear`; `Provider and model trigger has a dynamic accessible name; record the installed label`.

1. Open file mention suggestions and navigate with arrows; choose the fixture source file. Reopen and Escape.
2. Type /model and inspect the authorized model choices; dismiss. Inspect /side, /btw and /clear without executing them yet.

**Required visible outcome:** The chosen mention references the right file; Escape does not erase adjacent text. Suggestion selection and normal Enter submission are distinct. Dynamic provider commands are recorded only when present.

**Return and cleanup:** Remove exploratory mention/command tokens and return to the intended bundle A draft.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/web/src/components/chat/useComposerMenuItems.ts::useComposerMenuItems`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L61); [`apps/web/src/components/chat/ProviderModelPicker.tsx::ProviderModelPicker`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderModelPicker.tsx#L89); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C05 · Stash and restore structured content

**State:** phase 0; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-02, QUOTE-03.

**Entry labels and routes:** `Stashed prompts`; `Close stash`; `Restore stashed prompt: ${snippet}`; `Delete stashed prompt`; `Cmd+S is the default stash shortcut outside terminal focus`; `Source defaults only; inspect installed custom keymap`; `Cmd+F`; `Cmd+S`; `Cmd+Shift+T`; `Cmd+W`; `Cmd+Shift+Enter`; `Alt+ArrowUp`; `Cmd+Enter`; `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Preview ${image.name}`; `Draft attachment may not persist`; `Retry upload for ${name}`; `Remove ${name}`.

1. Prepare a draft with text and an attachment; later repeat with a real citation from bundle A. Use the observed stash control or default Cmd+S outside terminal focus.
2. Restore into an empty composer, then exercise restoration with existing unrelated text and record the actual merge/replacement rule.

**Required visible outcome:** Stashed prompts are individually reachable and retain attachment/citation identity and editability. Existing draft content is neither silently lost nor confused with a queued edit.

**Return and cleanup:** Delete only audit stash entries after recording, and restore the intended active draft.

**Sol:** Verify editable draft/history restoration using real navigation or a normal restart after work is quiescent. Codex native snapshot support is not proof of UI restore; live-only pending callbacks do not survive restart.

**Sonnet:** Verify the actual transcript/side-chat and editable draft after restore. Claude does not declare native snapshots or restart-surviving pending requests; record clean recovery or explicit unavailable state, not invented continuity.

**Owning source:** [`apps/web/src/components/chat/ComposerStashMenu.tsx::ComposerStashMenu`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerStashMenu.tsx#L32); [`apps/web/src/components/chat/useComposerStash.ts::useComposerStash`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerStash.ts#L73); [`packages/shared/src/keybindings.ts::DEFAULT_KEYBINDINGS`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L57); [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachmentMediaRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L136); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197); [`apps/web/src/composerDraftStore.ts::composerDraftStore`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2673).

#### T3-C06 · Attach supported files and images through the native picker

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** ATTACH-01.

**Entry labels and routes:** `Attach files`; `Use the actual picker, file drop or pasted image path`; `Preview ${image.name}`; `Draft attachment may not persist`; `Retry upload for ${name}`; `Remove ${name}`; `Attach again`; `Remove to send`; `Retry upload`; `Remove`.

1. Use Attach files to select audit-notes.md and audit-image.png. Record pending and ready states.
2. Open the picker again and cancel without choosing a file.

**Required visible outcome:** Both selected files appear with accurate names/types and ready states; picker cancellation leaves the draft intact. Send availability agrees with attachment readiness.

**Return and cleanup:** Retain these attachments for delivery verification; do not select any user document.

**Sol:** Verify a real Sol turn receives the distinct text, citation and attachment markers applicable to this case once. Model acknowledgement is semantic evidence, not byte-exact transport evidence.

**Sonnet:** Verify a real Sonnet turn receives the corresponding markers once; Claude attachment conversion and history differ from Codex. Do not inherit a Sol delivery result.

**Owning source:** [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L702); [`apps/web/src/components/chat/useComposerAttachmentIntake.ts::useComposerAttachmentIntake`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L62); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachmentMediaRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L136); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachmentFileList`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L430); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::toCodexInput`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L3559); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::makeClaudeUserMessageWithAttachments`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L2795).

#### T3-C07 · Drop a file and paste an image or text

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** ATTACH-01, SHELL-03.

**Entry labels and routes:** `Use the actual picker, file drop or pasted image path`; `Composer accessible name is passed by the owner; record the actual name`; `Preview ${image.name}`; `Draft attachment may not persist`; `Retry upload for ${name}`; `Remove ${name}`.

1. Drop a second scratch file onto the intended composer target. Paste a scratch image, then paste ordinary text with the composer focused.
2. Repeat a plain-text paste with the right panel open.

**Required visible outcome:** File/image intake creates the correct attachment once, while text enters the clicked composer. No duplicate upload, wrong-target paste or automatic send occurs.

**Return and cleanup:** Remove redundant fixture attachments and restore any clipboard content changed by the audit.

**Sol:** Verify a real Sol turn receives the distinct text, citation and attachment markers applicable to this case once. Model acknowledgement is semantic evidence, not byte-exact transport evidence.

**Sonnet:** Verify a real Sonnet turn receives the corresponding markers once; Claude attachment conversion and history differ from Codex. Do not inherit a Sol delivery result.

**Owning source:** [`apps/web/src/components/chat/useComposerAttachmentIntake.ts::useComposerAttachmentIntake`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L62); [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachmentMediaRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L136); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::toCodexInput`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L3559); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::makeClaudeUserMessageWithAttachments`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L2795).

#### T3-C08 · Preview and remove a draft attachment

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** ATTACH-01, SHELL-04.

**Entry labels and routes:** `Preview ${image.name}`; `Draft attachment may not persist`; `Retry upload for ${name}`; `Remove ${name}`; `File preview content is derived from the selected attachment`; `Close ${mediaLabel} preview`; `Previous media`; `Next media`.

1. Open the file and image previews through their visible entries. With multiple images, use Previous media and Next media, then Escape.
2. Remove one selected attachment and inspect remaining names and order.

**Required visible outcome:** The preview matches the selected fixture and remains readable. Keyboard focus returns to the initiating surface; removing one item leaves other attachments and draft text intact.

**Return and cleanup:** Leave only the attachments needed for bundle A and close the preview.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachmentMediaRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L136); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerFilePreviewDialog`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L534); [`apps/web/src/components/chat/ExpandedImageDialog.tsx::ExpandedImageDialog`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ExpandedImageDialog.tsx#L80); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C09 · Reach image overflow and real upload recovery states

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** ATTACH-01, COMPOSE-01.

**Entry labels and routes:** `Show ${overflowCount} more image attachments`; `Preview ${image.name}`; `Draft attachment may not persist`; `Retry upload for ${name}`; `Remove ${name}`; `Attach again`; `Remove to send`; `Retry upload`; `Remove`.

1. Add the small fixture image set until the overflow control appears, then reveal and navigate the hidden items.
2. If a real upload failure or nonpersistent draft state occurs, inspect its message and use the offered Retry upload, Attach again or Remove to send action. Do not force unrelated network disconnection.

**Required visible outcome:** Every added image remains reachable. Failure recovery, if reached, changes the affected item without duplicating ready items, and the send state is truthful. Unreached failure branches remain not-reached.

**Return and cleanup:** Remove overflow-only fixtures before sending, and leave no pending audit upload.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerRestingImagePreviews`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L584); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachmentMediaRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L136); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachmentFileList`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L430); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C10 · Send once with text, file and image context

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-01, ATTACH-01, ATTACH-02.

**Entry labels and routes:** `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`; `Send through the visible composer primary action`; `Preview ${label}`; `Download ${label}`; `Copy prompt link`.

1. Submit the prepared bundle A prompt once. Observe the sent row, attachment preview and turn start.
2. Ask the model to identify FILE_MARKER_T3 and the known image marker, then compare with the fixtures.

**Required visible outcome:** The intended prompt and attachments are represented once in the sent row and the real provider sees the requested content. Model acknowledgement supports semantic delivery only; byte-exact payload fidelity requires separate transport evidence.

**Return and cleanup:** Keep the running turn for live and queue cases; record native/thread IDs and all subsequent model turns.

**Sol:** Verify a real Sol turn receives the distinct text, citation and attachment markers applicable to this case once. Model acknowledgement is semantic evidence, not byte-exact transport evidence.

**Sonnet:** Verify a real Sonnet turn receives the corresponding markers once; Claude attachment conversion and history differ from Codex. Do not inherit a Sol delivery result.

**Owning source:** [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/web/src/components/chat/chatViewSend.ts::prepareTurnAttachments`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/chatViewSend.ts#L327); [`apps/web/src/components/chat/TimelineUserMessageRow.tsx::UserTimelineRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L433); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::toCodexInput`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L3559); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::makeClaudeUserMessageWithAttachments`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L2795); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C11 · Observe assistant output before completion

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** LIVE-01.

**Entry labels and routes:** `Load earlier turns`; `Loading earlier turns…`; `Manual scroll, live follow and scroll restoration have no standalone action label`.

1. Capture the first assistant content and at least one later increment while the task is still running.
2. Compare timestamps with the final state and inspect whether the same message grows coherently.

**Required visible outcome:** Readable content arrives before completion without duplicate messages, disappearing earlier text or a false completed state.

**Return and cleanup:** Leave the task running for the bounded tool and queue observations.

**Sol:** Codex declares assistant, reasoning, tool and plan streaming. Require an intermediate visible update before completion for each output type actually exercised.

**Sonnet:** Claude declares assistant/reasoning streaming but not generic tool/plan streaming. Its separate tailClaudeTaskOutput path may supply live background command output. Record first, intermediate and final observations for the actual path instead of declaring all tool output unavailable.

**Owning source:** [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/web/src/components/chat/useTimelineFollow.ts::useTimelineFollow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L166); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353).

#### T3-C12 · Observe live tool or background output on its actual route

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** LIVE-01, AGENT-04.

**Entry labels and routes:** `Activity`; `Tool calls`; `Individual tool headings are data dependent`; `Details`; `Background work`; `Waiting`; `Running`; `Open thread`; `Load earlier turns`; `Loading earlier turns…`.

1. Open the running bounded tick command before it completes. Capture early, intermediate and final output with timestamps.
2. If work is backgrounded, open Details and its owning thread; distinguish the main tool row from the background output surface.

**Required visible outcome:** Record exactly which route updates and when. Ordered tick markers, final result and owner remain coherent; an output surface that updates only on completion is reported as such. Generic streaming capability flags do not certify this case.

**Return and cleanup:** The command must finish or be explicitly stopped within its bound; never detach it.

**Sol:** Codex declares assistant, reasoning, tool and plan streaming. Require an intermediate visible update before completion for each output type actually exercised.

**Sonnet:** Claude declares assistant/reasoning streaming but not generic tool/plan streaming. Its separate tailClaudeTaskOutput path may supply live background command output. Record first, intermediate and final observations for the actual path instead of declaring all tool output unavailable.

**Owning source:** [`apps/web/src/components/chat/TimelineWorkGroupRows.tsx::ExpandedWorkGroupEntries`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineWorkGroupRows.tsx#L132); [`apps/web/src/components/chat/BackgroundWorkDetails.tsx::BackgroundWorkDetails`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L21); [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/server/src/orchestration-v2/Adapters/ClaudeTaskOutputTail.ts::tailClaudeTaskOutput`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeTaskOutputTail.ts#L61).

#### T3-C13 · Read rich output at normal and narrow widths

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** READ-03, SHELL-02, AGENT-01.

**Entry labels and routes:** `Load earlier turns`; `Loading earlier turns…`; `Wrap lines`; `Disable line wrap`; `Copy code`; `Copied`; `Code block actions`; `Run in terminal`.

1. Inspect the report prose, table, fenced code, literal blockquote and links at the initial width. Resize the right panel and return it.
2. Toggle Wrap lines and Disable line wrap for a long code line and read the response beginning, middle and end.

**Required visible outcome:** Content remains reachable without unexplained clipping or stranded horizontal position. The code toggle affects the intended block and panel resizing does not require a new model delta to repaint readable text.

**Return and cleanup:** Return to the starting panel width and code-wrap preference.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/web/src/components/chat/MarkdownCodeBlock.tsx::MarkdownCodeBlock`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownCodeBlock.tsx#L129); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C14 · Copy the intended message, code and prompt link

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** LINK-01, QUOTE-03.

**Entry labels and routes:** `Copy message`; `Wrap lines`; `Disable line wrap`; `Copy code`; `Copied`; `Code block actions`; `Run in terminal`; `Preview ${label}`; `Download ${label}`; `Copy prompt link`.

1. Use Copy message, Copy code and Copy prompt link on distinct targets. Paste each into a disposable scratch field or file for comparison.

**Required visible outcome:** The copied content matches the selected target, code remains intact, and the prompt link identifies the correct thread/message. A transient Copied label alone is insufficient evidence.

**Return and cleanup:** Remove copied scratch artifacts if no longer needed and restore the previous clipboard where practical.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/MessageCopyButton.tsx::MessageCopyButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessageCopyButton.tsx#L12); [`apps/web/src/components/chat/MarkdownCodeBlock.tsx::MarkdownCodeBlock`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownCodeBlock.tsx#L129); [`apps/web/src/components/chat/TimelineUserMessageRow.tsx::UserTimelineRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L433); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C15 · Open a link through its normal context menu

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** LINK-01, BROWSER-UI-02, SHELL-04.

**Entry labels and routes:** `Open in integrated browser`; `Open in system browser`; `Copy Link`.

1. Right-click the harmless example.com link. Use Open in integrated browser, return, then use Open in system browser and Copy Link.
2. Reopen the menu and dismiss it with Escape and an outside click.

**Required visible outcome:** Each action reaches or copies the exact URL selected. The system browser and integrated browser are distinguished, and menu dismissal does not activate an item or steal the draft.

**Return and cleanup:** Close only the browser views/tabs created by this audit and restore thread focus.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/externalLinkContextMenu.ts::showExternalLinkContextMenu`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/externalLinkContextMenu.ts#L88); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C16 · Follow a file reference and return to the same draft

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** FILES-05, SHELL-03.

**Entry labels and routes:** `Load earlier turns`; `Loading earlier turns…`; `Composer accessible name is passed by the owner; record the actual name`.

1. Activate the generated scratch source-file link or line reference. Inspect the file and requested line when specified.
2. Return to the thread, place a caret in its draft and type a marker with the file panel still open.

**Required visible outcome:** The intended file/location opens, and ordinary typing stays in the clicked composer. The original thread, draft and scroll position remain identifiable.

**Return and cleanup:** Remove the typed marker and close only the owned file view.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/web/src/components/chat/MarkdownFileLink.tsx::MarkdownFileLink`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownFileLink.tsx#L53).

#### T3-C17 · Keep the reading position during real append

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** READ-02, READ-03.

**Entry labels and routes:** `Manual scroll, live follow and scroll restoration have no standalone action label`; `Scroll to end`; `Load earlier turns`; `Loading earlier turns…`.

1. While output is still arriving, scroll to a named middle paragraph and record its position. Allow at least one new visible append.
2. Then activate Scroll to end and observe subsequent append behavior.

**Required visible outcome:** Manual reading position wins while away from the end. Scroll to end reaches the newest content and resumes the expected follow behavior without duplicate jumps.

**Return and cleanup:** Return to the newest output and preserve evidence for both detached and following states.

**Sol:** Codex declares assistant, reasoning, tool and plan streaming. Require an intermediate visible update before completion for each output type actually exercised.

**Sonnet:** Claude declares assistant/reasoning streaming but not generic tool/plan streaming. Its separate tailClaudeTaskOutput path may supply live background command output. Record first, intermediate and final observations for the actual path instead of declaring all tool output unavailable.

**Owning source:** [`apps/web/src/components/chat/useTimelineFollow.ts::useTimelineFollow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L166); [`apps/web/src/components/chat/TimelineOverlays.tsx::ScrollToEndButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineOverlays.tsx#L26); [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353).

#### T3-C18 · Page and resize without losing content or input ownership

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** READ-02, READ-03, SHELL-03, SHELL-02.

**Entry labels and routes:** `Manual scroll, live follow and scroll restoration have no standalone action label`; `Load earlier turns`; `Loading earlier turns…`; `Source defaults only; inspect installed custom keymap`; `Cmd+F`; `Cmd+S`; `Cmd+Shift+T`; `Cmd+W`; `Cmd+Shift+Enter`; `Alt+ArrowUp`; `Cmd+Enter`.

1. On a long expanded result, exercise PageUp, PageDown, End, thumb drag and wheel/trackpad scrolling as available.
2. Repeat around a sidebar/right-panel resize and interrupt an automatic movement with manual scrolling.

**Required visible outcome:** Content remains reachable, the active input surface receives the intended keys, and a manual movement is respected. A capture losing unrelated app chrome is inconclusive until independently confirmed in the native app.

**Return and cleanup:** Restore initial panel sizes and the intended reading position.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/useTimelineFollow.ts::useTimelineFollow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L166); [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`packages/shared/src/keybindings.ts::DEFAULT_KEYBINDINGS`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L57); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C19 · Find and reveal text inside folded history

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** READ-01, READ-04.

**Entry labels and routes:** `Find in thread`; `Previous match`; `Next match`; `Close find`; `Searching…`; `Search failed`; `Retry`; `Activate a real match through the Find controls`; `Activity`; `Tool calls`; `Individual tool headings are data dependent`; `Source defaults only; inspect installed custom keymap`; `Cmd+F`; `Cmd+S`; `Cmd+Shift+T`; `Cmd+W`; `Cmd+Shift+Enter`; `Alt+ArrowUp`; `Cmd+Enter`.

1. Fold a tool result containing TOOL_FIND_SENTINEL. Open Find in thread and search for it.
2. Use Next match and Previous match, inspect the highlighted source, then search for a known absent string and close Find.

**Required visible outcome:** A real match is revealed in the correct owning content or the documented search scope is visibly clear. Counts and current highlight agree, and the absent query does not retain a stale hit.

**Return and cleanup:** Close Find and restore the prior fold/reading state.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/ThreadFindBar.tsx::ThreadFindBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadFindBar.tsx#L26); [`apps/web/src/components/chat/useThreadFindNavigation.ts::revealThreadFindMatch`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadFindNavigation.ts#L35); [`apps/web/src/components/chat/TimelineWorkGroupRows.tsx::ExpandedWorkGroupEntries`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineWorkGroupRows.tsx#L132); [`packages/shared/src/keybindings.ts::DEFAULT_KEYBINDINGS`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L57); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C20 · Keep Find coherent through streaming and navigation

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** READ-01, READ-02.

**Entry labels and routes:** `Find in thread`; `Previous match`; `Next match`; `Close find`; `Searching…`; `Search failed`; `Retry`; `Activate a real match through the Find controls`; `Manual scroll, live follow and scroll restoration have no standalone action label`.

1. While text is arriving, search for an already visible marker and then a later marker as it appears.
2. Navigate to another owned thread and back with Find active, and inspect a hit inside a tall code/result row.

**Required visible outcome:** The query, match count, active highlight and scroll destination describe the current thread. New content does not leave a stale result or hide the active match.

**Return and cleanup:** Close Find and remove no conversation content.

**Sol:** Codex declares assistant, reasoning, tool and plan streaming. Require an intermediate visible update before completion for each output type actually exercised.

**Sonnet:** Claude declares assistant/reasoning streaming but not generic tool/plan streaming. Its separate tailClaudeTaskOutput path may supply live background command output. Record first, intermediate and final observations for the actual path instead of declaring all tool output unavailable.

**Owning source:** [`apps/web/src/components/chat/ThreadFindBar.tsx::ThreadFindBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadFindBar.tsx#L26); [`apps/web/src/components/chat/useThreadFindNavigation.ts::revealThreadFindMatch`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadFindNavigation.ts#L35); [`apps/web/src/components/chat/useTimelineFollow.ts::useTimelineFollow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L166); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353).

#### T3-C21 · Reach earlier messages through minimap and pagination

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** READ-02, READ-03.

**Entry labels and routes:** `Jump to message: ${prompt}`; `Load earlier turns`; `Loading earlier turns…`; `Scroll to end`.

1. Use Jump to message on an earlier prompt and inspect the exact destination.
2. If Load earlier turns is present, invoke it and navigate into the loaded history, then return with Scroll to end.

**Required visible outcome:** The target prompt is actually visible and pagination preserves continuity without duplicated or inaccessible rows. If the bounded history does not expose pagination, that branch remains not-reached.

**Return and cleanup:** Return to the newest owned conversation content.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/TimelineMinimap.tsx::TimelineMinimap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineMinimap.tsx#L32); [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/web/src/components/chat/TimelineOverlays.tsx::ScrollToEndButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineOverlays.tsx#L26); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C22 · Expand and collapse long tool work in place

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** READ-04, READ-02.

**Entry labels and routes:** `Activity`; `Tool calls`; `Individual tool headings are data dependent`; `Load earlier turns`; `Loading earlier turns…`; `Manual scroll, live follow and scroll restoration have no standalone action label`.

1. Open a completed work group and an individual long result. Read the first, middle and final marker.
2. Collapse and reopen it while another section remains open.

**Required visible outcome:** The intended result expands, full output stays reachable, unrelated expansion state is preserved, and the viewport does not jump to a different message.

**Return and cleanup:** Restore the original fold state after evidence capture.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/TimelineWorkGroupRows.tsx::ExpandedWorkGroupEntries`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineWorkGroupRows.tsx#L132); [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/web/src/components/chat/useTimelineFollow.ts::useTimelineFollow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L166); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C23 · Select assistant text and invoke Cite with mouse and keyboard

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-01, SHELL-03.

**Entry labels and routes:** `Cite selection in composer`; `Selection is too long to cite`; `Cite`; `Shorten selection`; `Composer accessible name is passed by the owner; record the actual name`.

1. Select a word, a multiline prose passage and a code passage in the assistant response in separate trials. Use the visible Cite action.
2. Repeat a selection with keyboard navigation; Tab toward the toolbar, then test Escape dismissal without citing.

**Required visible outcome:** The inserted citation matches the selected text exactly enough to distinguish the trials. Selection boundaries, focus and surrounding draft text remain intact; dismissing the toolbar does not create a citation.

**Return and cleanup:** Retain one useful citation for the comment/send cases and remove the exploratory ones.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/AssistantSelectionToolbar.tsx::AssistantSelectionToolbar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L23); [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C24 · Inspect the full source of an individual citation

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-02, READ-02.

**Entry labels and routes:** `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Cite selection in composer`; `Selection is too long to cite`; `Cite`; `Shorten selection`.

1. Create two citations from distinct passages. Activate View cited assistant text for each and inspect the full source, including any horizontally scrolled code/table location.
2. Return to the composer after each navigation.

**Required visible outcome:** Each citation reaches its own original passage and enough source text to understand it. The action does not silently substitute the whole response or another quote.

**Return and cleanup:** Return to the same draft and retain the intended citation set.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/chat/AssistantSelectionToolbar.tsx::AssistantSelectionToolbar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L23); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/web/src/lib/assistantCitationNavigation.ts::assistantCitationNavigation`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/assistantCitationNavigation.ts#L79).

#### T3-C25 · Add, edit, cancel and dismiss a citation comment

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-02, SHELL-04.

**Entry labels and routes:** `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Comment on selected text`; `Add an optional comment...`; `Cancel`; `Shorten comment`; `Save`.

1. Use Add comment to citation and enter a distinctive multiline comment. Save, reopen Edit citation comment and change it.
2. Separately exercise Cancel, Escape and outside dismissal; record which exits commit according to the actual path.

**Required visible outcome:** Saved comments remain attached to the correct citation and are editable. Cancel/Escape preserve the previous saved value. The source dismissal helper commits changed in-limit comments on outside dismissal, closes unchanged comments, and keeps oversized comments open; verify each branch actually reached.

**Return and cleanup:** Leave a short intentional saved comment and remove abandoned edit text.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/chat/AssistantCitationCommentEditor.tsx::AssistantCitationCommentEditor`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L10); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/web/src/components/chat/assistantCitationCommentDismissal.ts::resolveAssistantCitationCommentDismissal`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/assistantCitationCommentDismissal.ts#L8).

#### T3-C26 · Send a citation with both its own comment and general text

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-01, QUOTE-02.

**Entry labels and routes:** `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Comment on selected text`; `Add an optional comment...`; `Cancel`; `Shorten comment`; `Save`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`; `Send through the visible composer primary action`.

1. Add GENERAL_TEXT_T3 in the main composer and COMMENT_T3 on a citation. Use Enter in the comment editor to save without sending.
2. On a deliberate separate trial use Cmd/Ctrl+Enter in that editor to save and send once; inspect the sent row and ask for a short acknowledgement of the three distinct components.

**Required visible outcome:** Source quote, citation comment and general text all survive. Enter versus modified Enter has the documented distinction and no duplicate turn starts. A model acknowledgement is semantic evidence, not byte-exact payload proof.

**Return and cleanup:** Record every resulting turn and keep replies short; remove any leftover unsent trial draft.

**Sol:** Verify a real Sol turn receives the distinct text, citation and attachment markers applicable to this case once. Model acknowledgement is semantic evidence, not byte-exact transport evidence.

**Sonnet:** Verify a real Sonnet turn receives the corresponding markers once; Claude attachment conversion and history differ from Codex. Do not inherit a Sol delivery result.

**Owning source:** [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/chat/AssistantCitationCommentEditor.tsx::AssistantCitationCommentEditor`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L10); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/web/src/components/chat/chatViewSend.ts::prepareTurnAttachments`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/chatViewSend.ts#L327); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C27 · Edit multiple citations without crossing their identities

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-02, QUOTE-03, SHELL-03.

**Entry labels and routes:** `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Comment on selected text`; `Add an optional comment...`; `Cancel`; `Shorten comment`; `Save`; `Composer accessible name is passed by the owner; record the actual name`.

1. Cite two different passages and give each a distinct comment. Place the caret before, between and after them.
2. Delete one citation, undo the deletion, and edit the second comment.

**Required visible outcome:** Text insertion, delete/undo and comment editing preserve each citation/source pairing. No unrelated general text or second citation is lost.

**Return and cleanup:** Remove exploratory citations or keep only the intended next prompt.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/chat/AssistantCitationCommentEditor.tsx::AssistantCitationCommentEditor`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L10); [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C28 · Exercise citation and comment length boundaries honestly

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-01, QUOTE-02.

**Entry labels and routes:** `Cite selection in composer`; `Selection is too long to cite`; `Cite`; `Shorten selection`; `Comment on selected text`; `Add an optional comment...`; `Cancel`; `Shorten comment`; `Save`.

1. If the real response is long enough, select more than 8000 characters and inspect Shorten selection, then shorten to an accepted selection.
2. In a comment editor try 8001 characters, inspect Shorten comment, then reduce to 8000 and cancel after verifying readiness.

**Required visible outcome:** Oversized content has an explicit non-sending state; accepted content can be edited without truncation. The source selection branch remains not-reached when the available response is too short, instead of generating artificial evidence.

**Return and cleanup:** Cancel the boundary comment and remove the oversized selection; do not send the oversized fixture.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/AssistantSelectionToolbar.tsx::AssistantSelectionToolbar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L23); [`apps/web/src/components/chat/AssistantCitationCommentEditor.tsx::AssistantCitationCommentEditor`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L10); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`packages/contracts/src/assistantCitations.ts::ASSISTANT_CITATION_MAX_TEXT_LENGTH`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/assistantCitations.ts#L4); [`packages/contracts/src/assistantCitations.ts::ASSISTANT_CITATION_MAX_COMMENT_LENGTH`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/assistantCitations.ts#L5).

#### T3-C29 · Keep literal blockquote text distinct from a citation through restore

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-03, COMPOSE-02, ATTACH-02.

**Entry labels and routes:** `Composer accessible name is passed by the owner; record the actual name`; `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Stashed prompts`; `Close stash`; `Restore stashed prompt: ${snippet}`; `Delete stashed prompt`; `Restore the editable prompt and attachments after rewind`.

1. Prepare a draft containing a literal leading > line, ordinary prose, a real citation and a fixture attachment. Stash/restore and navigate away/back.
2. Recheck after the later normal restart, then edit the literal line and citation comment separately.

**Required visible outcome:** Plain text formatting remains editable as text, while intentional citations preserve their source identity. No extra citation tile or silent literal-text loss appears.

**Return and cleanup:** Preserve the draft only until the recovery evidence is complete, then clear owned content.

**Sol:** Verify editable draft/history restoration using real navigation or a normal restart after work is quiescent. Codex native snapshot support is not proof of UI restore; live-only pending callbacks do not survive restart.

**Sonnet:** Verify the actual transcript/side-chat and editable draft after restore. Claude does not declare native snapshots or restart-surviving pending requests; record clean recovery or explicit unavailable state, not invented continuity.

**Owning source:** [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/chat/ComposerStashMenu.tsx::ComposerStashMenu`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerStashMenu.tsx#L32); [`apps/web/src/components/chat/chatViewSend.ts::restoreComposerDraft`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/chatViewSend.ts#L257); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197); [`apps/web/src/composerDraftStore.ts::composerDraftStore`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2673); [`apps/web/src/composerDraftStore.ts::partializeComposerDraftStoreState`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2246); [`apps/web/src/composerDraftStore.ts::hydrateImagesFromPersisted`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2557).

#### T3-C30 · Reply to a whole response without losing the user annotation

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-01, QUOTE-02.

**Entry labels and routes:** `Reply to this response`; `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`.

1. Use Reply to this response on a known assistant response. Inspect the inserted response context and add WHOLE_REPLY_T3 as general text.
2. Send a short acknowledgement request and inspect the sent representation.

**Required visible outcome:** The response context and the user annotation both reach the intended turn once. The context can be identified from the composer and sent row.

**Return and cleanup:** Record the extra short turn and leave no unintended reply context in the composer.

**Sol:** Verify a real Sol turn receives the distinct text, citation and attachment markers applicable to this case once. Model acknowledgement is semantic evidence, not byte-exact transport evidence.

**Sonnet:** Verify a real Sonnet turn receives the corresponding markers once; Claude attachment conversion and history differ from Codex. Do not inherit a Sol delivery result.

**Owning source:** [`apps/web/src/components/chat/TimelineAssistantRows.tsx::AssistantReplyButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineAssistantRows.tsx#L220); [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C31 · Queue several prompts while the active task runs

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-03, COMPOSE-04.

**Entry labels and routes:** `${items.length} queued messages`; `Queued message ${position + 1} of ${items.length}, not sent`; `Send now`; `Editing in the composer`; `Cancel editing queued message`; `Edit queued message`; `Remove queued message`; `Remove from queue`; `Show all ${items.length} queued`; `Show fewer`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`.

1. While the bounded command is running, inspect the current Queue message/Steer message behavior before using it. Queue short markers Q1_T3 through Q4_T3.
2. Capture queue count, order and not sent labels before any queued prompt runs.

**Required visible outcome:** Each marker creates exactly one queued entry in the expected order. A queue action does not silently steer or immediately send because of an assumed modifier/default.

**Return and cleanup:** Remove unused entries before automatic continuation; keep only the targeted items for the next cases.

**Sol:** Codex supports queueing, active turn/steer and interruption. Prove exact marker identity, cardinality and order at the exercised transition.

**Sonnet:** Claude supports queueing and active steering; steering may interrupt tools and background foreground subagents. Check the visible owner, accepted prompt and resulting work. No assumption of Codex interruption behavior.

**Owning source:** [`apps/web/src/components/chat/QueuedRunsControl.tsx::QueuedRunsControl`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L77); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::steerTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7844); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::steerTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12873); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C32 · Reach every queued item through overflow

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-03.

**Entry labels and routes:** `${items.length} queued messages`; `Queued message ${position + 1} of ${items.length}, not sent`; `Send now`; `Editing in the composer`; `Cancel editing queued message`; `Edit queued message`; `Remove queued message`; `Remove from queue`; `Show all ${items.length} queued`; `Show fewer`.

1. With four queued items, use Show all 4 queued, reach the last item with pointer and keyboard, then use Show fewer.

**Required visible outcome:** Every queue entry remains individually operable and clearly identified. Collapsing the view preserves the queue and selected/editing state.

**Return and cleanup:** Leave the queue visible for editing and prevent unneeded items from executing.

**Sol:** Codex supports queueing, active turn/steer and interruption. Prove exact marker identity, cardinality and order at the exercised transition.

**Sonnet:** Claude supports queueing and active steering; steering may interrupt tools and background foreground subagents. Check the visible owner, accepted prompt and resulting work. No assumption of Codex interruption behavior.

**Owning source:** [`apps/web/src/components/chat/QueuedRunsControl.tsx::QueuedRunsControl`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L77); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353).

#### T3-C33 · Cancel a queued edit while another draft exists

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-03.

**Entry labels and routes:** `${items.length} queued messages`; `Queued message ${position + 1} of ${items.length}, not sent`; `Send now`; `Editing in the composer`; `Cancel editing queued message`; `Edit queued message`; `Remove queued message`; `Remove from queue`; `Show all ${items.length} queued`; `Show fewer`; `Queued message is no longer queued`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`.

1. Enter UNRELATED_DRAFT_T3 in the composer. Use Edit queued message on Q1_T3, change it, then Cancel editing queued message.
2. Inspect both the queued entry and the unrelated draft.

**Required visible outcome:** Cancel restores the original queue item and the unrelated draft as separate objects. It does not remove the queue item or merge both texts into one draft.

**Return and cleanup:** Keep the unrelated draft for the update trial and remove abandoned edit text.

**Sol:** Codex supports queueing, active turn/steer and interruption. Prove exact marker identity, cardinality and order at the exercised transition.

**Sonnet:** Claude supports queueing and active steering; steering may interrupt tools and background foreground subagents. Check the visible owner, accepted prompt and resulting work. No assumption of Codex interruption behavior.

**Owning source:** [`apps/web/src/components/chat/QueuedRunsControl.tsx::QueuedRunsControl`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L77); [`apps/web/src/components/chat/useQueuedRunEdit.ts::useQueuedRunEdit`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useQueuedRunEdit.ts#L195); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353).

#### T3-C34 · Update one queued message including an attachment

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-03, ATTACH-01.

**Entry labels and routes:** `${items.length} queued messages`; `Queued message ${position + 1} of ${items.length}, not sent`; `Send now`; `Editing in the composer`; `Cancel editing queued message`; `Edit queued message`; `Remove queued message`; `Remove from queue`; `Show all ${items.length} queued`; `Show fewer`; `Queued message is no longer queued`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`; `Attach files`.

1. Edit Q1_T3 again, change its text and attach one small fixture. Use Update queued message.
2. Inspect Q1, the other queued entries, ordering and the unrelated composer draft. If a real rejection occurs, inspect recoverability.

**Required visible outcome:** Only the intended queued item changes, exactly once, with its attachment and position preserved. Unrelated queue items and draft remain intact. A rejected update retains recoverable edits.

**Return and cleanup:** Remove unused queue entries and restore the intended composer draft.

**Sol:** Codex supports queueing, active turn/steer and interruption. Prove exact marker identity, cardinality and order at the exercised transition.

**Sonnet:** Claude supports queueing and active steering; steering may interrupt tools and background foreground subagents. Check the visible owner, accepted prompt and resulting work. No assumption of Codex interruption behavior.

**Owning source:** [`apps/web/src/components/chat/QueuedRunsControl.tsx::QueuedRunsControl`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L77); [`apps/web/src/components/chat/useQueuedRunEdit.ts::useQueuedRunEdit`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useQueuedRunEdit.ts#L195); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L702); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::toCodexInput`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L3559); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::makeClaudeUserMessageWithAttachments`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L2795); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C35 · Remove and promote the exact queued item

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-03, COMPOSE-04.

**Entry labels and routes:** `${items.length} queued messages`; `Queued message ${position + 1} of ${items.length}, not sent`; `Send now`; `Editing in the composer`; `Cancel editing queued message`; `Edit queued message`; `Remove queued message`; `Remove from queue`; `Show all ${items.length} queued`; `Show fewer`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`.

1. Remove Q4_T3 through its own control. Then use Send now for the intended Q1_T3 entry while observing the active turn and remaining queue.

**Required visible outcome:** Removal affects only Q4. Promotion delivers Q1 exactly once and its ownership/status are clear; remaining entries are neither reordered unexpectedly nor merged.

**Return and cleanup:** Immediately remove any unused queue items and record every actual provider turn.

**Sol:** Codex supports queueing, active turn/steer and interruption. Prove exact marker identity, cardinality and order at the exercised transition.

**Sonnet:** Claude supports queueing and active steering; steering may interrupt tools and background foreground subagents. Check the visible owner, accepted prompt and resulting work. No assumption of Codex interruption behavior.

**Owning source:** [`apps/web/src/components/chat/QueuedRunsControl.tsx::QueuedRunsControl`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L77); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::steerTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7844); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::steerTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12873); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C36 · Steer active work through the observed alternate action

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-04, LIVE-01, AGENT-04.

**Entry labels and routes:** `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`; `Source defaults only; inspect installed custom keymap`; `Cmd+F`; `Cmd+S`; `Cmd+Shift+T`; `Cmd+W`; `Cmd+Shift+Enter`; `Alt+ArrowUp`; `Cmd+Enter`; `Running, queued, interrupted and resumed states are data dependent`; `Details`; `Background work`; `Waiting`; `Running`; `Open thread`.

1. Inspect the primary action tooltip while running. Send STEER_T3 using the actual alternate click/key action shown, with a short instruction to acknowledge once and keep all scratch limits.
2. Observe the active tool, foreground/background ownership and remaining queue.

**Required visible outcome:** The steering message is accepted once in the intended active conversation. Tool interruption/background behavior matches the actual provider path and the UI makes the resulting ownership clear.

**Return and cleanup:** Do not restart interrupted work automatically; remove unused queued content and account for any background work.

**Sol:** Codex supports queueing, active turn/steer and interruption. Prove exact marker identity, cardinality and order at the exercised transition.

**Sonnet:** Claude supports queueing and active steering; steering may interrupt tools and background foreground subagents. Check the visible owner, accepted prompt and resulting work. No assumption of Codex interruption behavior.

**Owning source:** [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`packages/shared/src/keybindings.ts::DEFAULT_KEYBINDINGS`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L57); [`apps/web/src/components/chat/useThreadLifecycle.tsx::useThreadLifecycle`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L144); [`apps/web/src/components/chat/BackgroundWorkDetails.tsx::BackgroundWorkDetails`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L21); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::steerTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7844); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::interruptTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7920); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::steerTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12873); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::interruptTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12722); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C37 · Stop the active task and verify it actually stops

**State:** phase A; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-04, AGENT-02.

**Entry labels and routes:** `Stop generation`; `Stopping`; `Running, queued, interrupted and resumed states are data dependent`; `${items.length} queued messages`; `Queued message ${position + 1} of ${items.length}, not sent`; `Send now`; `Editing in the composer`; `Cancel editing queued message`; `Edit queued message`; `Remove queued message`; `Remove from queue`; `Show all ${items.length} queued`; `Show fewer`; `Details`; `Background work`; `Waiting`; `Running`; `Open thread`.

1. During the bounded foreground command, click Stop generation and observe the pending state. Test a repeated click only while the same stop request is pending.
2. Inspect the command, assistant state, queued prompts and any background task after the stop settles.

**Required visible outcome:** The control targets the intended work, reports a truthful final state and prevents duplicate stop/resume activity. Command liveness is checked independently of the spinner; queued continuation is explicitly observed.

**Return and cleanup:** Remove unused queued prompts and stop any owned descendant/command still alive through the parent-controlled cleanup path.

**Sol:** Codex supports queueing, active turn/steer and interruption. Prove exact marker identity, cardinality and order at the exercised transition.

**Sonnet:** Claude supports queueing and active steering; steering may interrupt tools and background foreground subagents. Check the visible owner, accepted prompt and resulting work. No assumption of Codex interruption behavior.

**Owning source:** [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::StopGenerationButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L132); [`apps/web/src/components/chat/useThreadLifecycle.tsx::useThreadLifecycle`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L144); [`apps/web/src/components/chat/QueuedRunsControl.tsx::QueuedRunsControl`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L77); [`apps/web/src/components/chat/BackgroundWorkDetails.tsx::BackgroundWorkDetails`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L21); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::interruptTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7920); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::interruptTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12722).

#### T3-C38 · Resume only the intended work after a stop

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-04, RESTORE-01.

**Entry labels and routes:** `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`; `${items.length} queued messages`; `Queued message ${position + 1} of ${items.length}, not sent`; `Send now`; `Editing in the composer`; `Cancel editing queued message`; `Edit queued message`; `Remove queued message`; `Remove from queue`; `Show all ${items.length} queued`; `Show fewer`; `Running, queued, interrupted and resumed states are data dependent`.

1. After the stopped state is settled, inspect Resume thread or the available send action. Resume with one short RECOVERY_T3_C message or promote one intended queued item.
2. Observe the resulting turn and queue.

**Required visible outcome:** Exactly the chosen work resumes with coherent history. Previously removed prompts and stopped commands do not reappear or restart unexpectedly.

**Return and cleanup:** Allow the short reply to finish, then empty the owned queue.

**Sol:** Codex supports queueing, active turn/steer and interruption. Prove exact marker identity, cardinality and order at the exercised transition.

**Sonnet:** Claude supports queueing and active steering; steering may interrupt tools and background foreground subagents. Check the visible owner, accepted prompt and resulting work. No assumption of Codex interruption behavior.

**Owning source:** [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/web/src/components/chat/QueuedRunsControl.tsx::QueuedRunsControl`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L77); [`apps/web/src/components/chat/useThreadLifecycle.tsx::useThreadLifecycle`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L144); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C39 · Make the permission label agree with the effective session

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** PERM-01, SETTINGS-01.

**Entry labels and routes:** `Permissions: Supervised`; `Permissions: Auto-accept edits`; `Permissions: Auto`; `Permissions: Full access`; `Supervised`; `Ask before commands and file changes.`; `Auto-accept edits`; `Auto`; `Full access`; `Approve`; `Decline`; `More approval options`; `Cancel`; `Always allow this session`.

1. Set Permissions: Supervised before starting the bundle B session. Request the harmless scratch action and inspect the real approval.
2. Inspect a mode change on a disposable owned session only if the UI offers it, then check the next action rather than assuming the label changed the current provider session.

**Required visible outcome:** The requested action follows the visible effective policy. Any deferred, disabled or new-session requirement is explicit and matches the provider contract.

**Return and cleanup:** Restore the original permission setting after all owned work has ended.

**Sol:** Set Supervised before the task and inspect the actual request. Codex maps supervised execution to approval-required policy; distinguish Approve, Decline and Cancel and observe the tool outcome.

**Sonnet:** Start the task in Supervised because Claude cannot change runtime mode within a native session. Observe the real permission request and response, not merely a changed menu label. Session grants must not leak beyond owned work.

**Owning source:** [`apps/web/src/components/chat/ComposerModelEffort.tsx::PermissionsMenu`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerModelEffort.tsx#L618); [`apps/web/src/components/chat/runtimeModeConfig.ts::runtimeModeConfig`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/runtimeModeConfig.ts#L14); [`apps/web/src/components/chat/ComposerPendingApprovalActions.tsx::ComposerPendingApprovalActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L33); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8352); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13484).

#### T3-C40 · Approve one harmless action and verify its exact result

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** PERM-01.

**Entry labels and routes:** `Approve`; `Decline`; `More approval options`; `Cancel`; `Always allow this session`; `Permissions: Supervised`; `Permissions: Auto-accept edits`; `Permissions: Auto`; `Permissions: Full access`.

1. Trigger a real approval for a scratch-only printf or creation of audit-write.txt. Inspect command/path, warning, owner and available choices.
2. Activate Approve once and inspect the resulting command output or file.

**Required visible outcome:** Only the displayed authorized action executes once. The request leaves pending state and the tool result reflects the actual outcome.

**Return and cleanup:** Retain the harmless fixture for later rewind checks or delete it only as part of owned scratch cleanup.

**Sol:** Set Supervised before the task and inspect the actual request. Codex maps supervised execution to approval-required policy; distinguish Approve, Decline and Cancel and observe the tool outcome.

**Sonnet:** Start the task in Supervised because Claude cannot change runtime mode within a native session. Observe the real permission request and response, not merely a changed menu label. Session grants must not leak beyond owned work.

**Owning source:** [`apps/web/src/components/chat/ComposerPendingApprovalActions.tsx::ComposerPendingApprovalActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L33); [`apps/web/src/components/chat/ComposerModelEffort.tsx::PermissionsMenu`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerModelEffort.tsx#L618); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8352); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13484).

#### T3-C41 · Decline a separate request without routing ordinary typing into approval

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** PERM-01, SHELL-03.

**Entry labels and routes:** `Approve`; `Decline`; `More approval options`; `Cancel`; `Always allow this session`; `Composer accessible name is passed by the owner; record the actual name`.

1. Trigger a second harmless approval. Click the main composer and type yes/no as unsent ordinary text, then inspect the still-pending request.
2. Explicitly choose Decline and observe the provider response.

**Required visible outcome:** Typing in the composer does not implicitly approve the action. Decline resolves the intended request and the denied operation does not execute.

**Return and cleanup:** Remove the unsent yes/no draft and clear only the resolved owned request.

**Sol:** Set Supervised before the task and inspect the actual request. Codex maps supervised execution to approval-required policy; distinguish Approve, Decline and Cancel and observe the tool outcome.

**Sonnet:** Start the task in Supervised because Claude cannot change runtime mode within a native session. Observe the real permission request and response, not merely a changed menu label. Session grants must not leak beyond owned work.

**Owning source:** [`apps/web/src/components/chat/ComposerPendingApprovalActions.tsx::ComposerPendingApprovalActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L33); [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8352); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13484).

#### T3-C42 · Inspect alternate approval choices and cancellation

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** PERM-01, SHELL-04.

**Entry labels and routes:** `Approve`; `Decline`; `More approval options`; `Cancel`; `Always allow this session`.

1. Open More approval options with mouse and keyboard, inspect the available labels/warnings and dismiss with Escape.
2. Exercise Cancel on an owned pending request when offered. Inspect session-grant wording; do not select a broad grant merely to tick a box.

**Required visible outcome:** Menu focus and dismissal are predictable. Cancel is distinguishable from Decline and approval. A session grant, if deliberately exercised on disposable work, has its actual scope recorded and the session is ended afterwards.

**Return and cleanup:** Resolve every audit request and leave no untracked grant or pending callback.

**Sol:** Set Supervised before the task and inspect the actual request. Codex maps supervised execution to approval-required policy; distinguish Approve, Decline and Cancel and observe the tool outcome.

**Sonnet:** Start the task in Supervised because Claude cannot change runtime mode within a native session. Observe the real permission request and response, not merely a changed menu label. Session grants must not leak beyond owned work.

**Owning source:** [`apps/web/src/components/chat/ComposerPendingApprovalActions.tsx::ComposerPendingApprovalActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L33); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8352); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13484).

#### T3-C43 · Answer real structured questions with required and free-text input

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** PERM-01, SHELL-03.

**Entry labels and routes:** `Question title and choices come from the provider; record the actual labels`; `Next question`; `Submit answers`; `Submit answer`; `Previous question`; `Submit`; `Next`; `Previous`; `Composer accessible name is passed by the owner; record the actual name`.

1. Ask the provider to use its actual structured question tool for two questions, one choice and one free-text answer. Submit empty first when validation is supported.
2. Select/type the answers, move Next question and Previous question, then Submit answers once.

**Required visible outcome:** The intended fields receive focus and ordinary typing, required validation is visible, previous answers persist, and the model receives the exact chosen/free-text values. Prose-only questions or a different approval are not this path.

**Return and cleanup:** Resolve the questions, clear any mistakenly entered main-composer scratch text and record unavailable tooling explicitly.

**Sol:** Require actual structured requestUserInput and verify question IDs, required options/free text and the submitted answer. Prose questions are not a pass.

**Sonnet:** Require AskUserQuestion through canUseToolEffect or a real elicitation through onElicitationEffect. Exercise the actual schema and returned answers; a prose question or unrelated approval does not satisfy this branch.

**Owning source:** [`apps/web/src/components/chat/ComposerPendingUserInputPanel.tsx::ComposerPendingUserInputPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingUserInputPanel.tsx#L30); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::formatPendingPrimaryActionLabel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L70); [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::toCodexUserInputAnswers`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L669); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8352); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::canUseToolEffect`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L11486); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::onElicitationEffect`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L11754); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13484).

#### T3-C44 · Preserve and dismiss the active question state coherently

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** PERM-01, RESTORE-01.

**Entry labels and routes:** `Question title and choices come from the provider; record the actual labels`; `Next question`; `Submit answers`; `Submit answer`; `Previous question`; `Submit`; `Next`; `Previous`.

1. While a structured question is pending, enter an answer and use its available collapse/navigation controls. Return and inspect the answer.
2. If a real cancel/dismiss action is present, exercise it separately from submitting an empty answer.

**Required visible outcome:** The request keeps its owner and answers through the exercised UI transition. Dismiss/cancel and valid submission have different visible outcomes; unsupported actions are not invented.

**Return and cleanup:** Resolve or cancel the owned request before closing its thread or restarting the app.

**Sol:** Require actual structured requestUserInput and verify question IDs, required options/free text and the submitted answer. Prose questions are not a pass.

**Sonnet:** Require AskUserQuestion through canUseToolEffect or a real elicitation through onElicitationEffect. Exercise the actual schema and returned answers; a prose question or unrelated approval does not satisfy this branch.

**Owning source:** [`apps/web/src/components/chat/ComposerPendingUserInputPanel.tsx::ComposerPendingUserInputPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingUserInputPanel.tsx#L30); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::formatPendingPrimaryActionLabel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L70); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::toCodexUserInputAnswers`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L669); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8352); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::canUseToolEffect`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L11486); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::onElicitationEffect`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L11754); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13484).

#### T3-C45 · Open a real child while it is producing work

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-01, AGENT-04.

**Entry labels and routes:** `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`; `Open parent`; `Runs on its own`; `Load earlier turns`; `Loading earlier turns…`.

1. Create only the bounded named audit child requested in bundle B. Record actual model, native/projected IDs and parent.
2. Open it from Lineage or its timeline entry before completion and inspect a live update, then use Open parent.

**Required visible outcome:** The entry opens the intended child with readable live content and the correct relationship. Returning reaches its immediate parent, preserving the parent draft/viewport. A generic Background work panel alone does not satisfy child navigation.

**Return and cleanup:** Keep an explicit list of every audit descendant and ensure each finishes or is stopped.

**Sol:** Codex exposes native child IDs, snapshots, waits, close and child fork capabilities. Prove the correct child/parent IDs and live content, with the authorized model recorded.

**Sonnet:** Claude does not declare native child thread IDs, native snapshot/child fork or child approvals, but the app projects child work and stopSubagent can call a live owning query. Verify the observed projected/read-only surface and stop result rather than assuming the Codex model.

**Owning source:** [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/web/src/components/chat/ProviderSubagentBar.tsx::ProviderSubagentBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14); [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8428); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13531); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::stopSubagent`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13419).

#### T3-C46 · Follow nested lineage without confusing the root and parent

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-01.

**Entry labels and routes:** `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`; `Open parent`; `Runs on its own`; `Open ${title} chat`; `Stop workflow ${label}`; `Stop entire workflow`; `Expand ${label}`; `Collapse ${label}`.

1. Only if the bounded provider task actually creates the permitted grandchild, open it through the child and inspect its relationship labels.
2. Use Open parent from grandchild, then from child, and compare IDs with the root.

**Required visible outcome:** Immediate-parent and root relationships are accurate and each transition opens the intended live/history surface. If the provider does not expose or create this branch, mark it not-reached with the observed reason.

**Return and cleanup:** Do not create extra descendants to fill the matrix; finish or stop the existing ones.

**Sol:** Codex exposes native child IDs, snapshots, waits, close and child fork capabilities. Prove the correct child/parent IDs and live content, with the authorized model recorded.

**Sonnet:** Claude does not declare native child thread IDs, native snapshot/child fork or child approvals, but the app projects child work and stopSubagent can call a live owning query. Verify the observed projected/read-only surface and stop result rather than assuming the Codex model.

**Owning source:** [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/web/src/components/chat/ProviderSubagentBar.tsx::ProviderSubagentBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14); [`apps/web/src/components/chat/ThreadLineageWorkflowRow.tsx::ThreadLineageWorkflowRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadLineageWorkflowRow.tsx#L120); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8428); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13531); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::stopSubagent`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13419).

#### T3-C47 · Read complete child prose at the available width

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-01, SHELL-02.

**Entry labels and routes:** `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`; `Open parent`; `Runs on its own`; `Load earlier turns`; `Loading earlier turns…`.

1. Read the first, middle and final sentence of the child response at the normal agent-panel width. Resize narrower and wider, then return.
2. Check before and after completion without waiting for a new delta to repair the view.

**Required visible outcome:** Full prose remains readable and reachable; clipping, missing lines or a view wider than its panel are recorded with native screenshots and dimensions.

**Return and cleanup:** Restore panel size and return to the parent.

**Sol:** Codex exposes native child IDs, snapshots, waits, close and child fork capabilities. Prove the correct child/parent IDs and live content, with the authorized model recorded.

**Sonnet:** Claude does not declare native child thread IDs, native snapshot/child fork or child approvals, but the app projects child work and stopSubagent can call a live owning query. Verify the observed projected/read-only surface and stop result rather than assuming the Codex model.

**Owning source:** [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/web/src/components/chat/ProviderSubagentBar.tsx::ProviderSubagentBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14); [`apps/web/src/components/chat/MessagesTimeline.tsx::MessagesTimeline`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L330); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8428); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13531).

#### T3-C48 · Open the correct background work and live output

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-04, LIVE-01.

**Entry labels and routes:** `Details`; `Background work`; `Waiting`; `Running`; `Open thread`; `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`.

1. Open Details for a known running child or command. Compare title, owner, status and output markers with the source work.
2. Use Open thread and return; inspect the transition from Waiting/Running to completion.

**Required visible outcome:** The background surface identifies and opens the correct work, reports actual state, and shows updates on the supported route. An empty surface is not excused without checking whether output was actually emitted.

**Return and cleanup:** Return to the parent and confirm no unexpected work remains running.

**Sol:** Codex declares assistant, reasoning, tool and plan streaming. Require an intermediate visible update before completion for each output type actually exercised.

**Sonnet:** Claude declares assistant/reasoning streaming but not generic tool/plan streaming. Its separate tailClaudeTaskOutput path may supply live background command output. Record first, intermediate and final observations for the actual path instead of declaring all tool output unavailable.

**Owning source:** [`apps/web/src/components/chat/BackgroundWorkDetails.tsx::BackgroundWorkDetails`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L21); [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeTaskOutputTail.ts::tailClaudeTaskOutput`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeTaskOutputTail.ts#L61); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::stopSubagent`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13419).

#### T3-C49 · Inspect native read-only subagent work as its own surface

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-01, AGENT-03.

**Entry labels and routes:** `Subagent work`; `Read only. These subagents cannot take messages.`; `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`.

1. When Subagent work is available, open it, inspect the Read only notice, native records and refresh behavior. Compare with the editable/projected child route if both exist.

**Required visible outcome:** Read-only scope is explicit and records belong to the intended work. Absence of a composer is not mislabeled a failure on this route; stale/no-record/error states are recorded separately.

**Return and cleanup:** Close the dialog and return to the same parent; do not create extra child work solely to force this surface.

**Sol:** Codex exposes native child IDs, snapshots, waits, close and child fork capabilities. Prove the correct child/parent IDs and live content, with the authorized model recorded.

**Sonnet:** Claude does not declare native child thread IDs, native snapshot/child fork or child approvals, but the app projects child work and stopSubagent can call a live owning query. Verify the observed projected/read-only surface and stop result rather than assuming the Codex model.

**Owning source:** [`apps/web/src/components/chat/NativeChildWorkDialog.tsx::NativeChildWorkDialog`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/NativeChildWorkDialog.tsx#L30); [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8428); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13531).

#### T3-C50 · Stop the selected child or whole workflow with truthful results

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-02, AGENT-04.

**Entry labels and routes:** `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`; `Open ${title} chat`; `Stop workflow ${label}`; `Stop entire workflow`; `Expand ${label}`; `Collapse ${label}`; `Details`; `Background work`; `Waiting`; `Running`; `Open thread`.

1. While an owned child is active, use its Stop subagent action and inspect that child and its parent.
2. If a real workflow group is present, exercise Stop entire workflow on the remaining owned workflow and inspect every member.

**Required visible outcome:** The requested scope matches the stopped work and unrelated threads continue unchanged. The UI reports completion/failure truthfully; a missing live owner or unsupported path is explicit rather than a false success.

**Return and cleanup:** Independently account for all audit root/child/grandchild tasks and commands before leaving bundle B.

**Sol:** Codex exposes native child IDs, snapshots, waits, close and child fork capabilities. Prove the correct child/parent IDs and live content, with the authorized model recorded.

**Sonnet:** Claude does not declare native child thread IDs, native snapshot/child fork or child approvals, but the app projects child work and stopSubagent can call a live owning query. Verify the observed projected/read-only surface and stop result rather than assuming the Codex model.

**Owning source:** [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/web/src/components/chat/ThreadLineageWorkflowRow.tsx::ThreadLineageWorkflowRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadLineageWorkflowRow.tsx#L120); [`apps/web/src/components/chat/BackgroundWorkDetails.tsx::BackgroundWorkDetails`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L21); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::interruptTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7920); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::interruptTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12722); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::stopSubagent`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13419).

#### T3-C51 · Respond to a child decision from the parent context

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-02, PERM-01.

**Entry labels and routes:** `Approval from ${title}`; `Approve`; `Decline`; `More approval options`; `Cancel`; `Always allow this session`; `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`.

1. If a real owned child emits an approval, leave the main parent focused, inspect Approval from with its title and act once.
2. Open the child and verify the corresponding operation result.

**Required visible outcome:** The decision resolves only the identified child request and remains visible in the correct context. Sol support is exercised when reachable; Claude unsupported child-approval capability is recorded with observed UI evidence, not simulated as an ordinary root request.

**Return and cleanup:** Resolve all pending child decisions before stopping or closing the child.

**Sol:** Codex exposes native child IDs, snapshots, waits, close and child fork capabilities. Prove the correct child/parent IDs and live content, with the authorized model recorded.

**Sonnet:** Claude does not declare native child thread IDs, native snapshot/child fork or child approvals, but the app projects child work and stopSubagent can call a live owning query. Verify the observed projected/read-only surface and stop result rather than assuming the Codex model.

**Owning source:** [`apps/web/src/components/chat/ChildThreadApprovals.tsx::ChildThreadApprovals`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChildThreadApprovals.tsx#L78); [`apps/web/src/components/chat/ComposerPendingApprovalActions.tsx::ComposerPendingApprovalActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L33); [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8352); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::respondToRuntimeRequest`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13484).

#### T3-C52 · Reopen completed agent history without restarting it

**State:** phase B; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-03, AGENT-04.

**Entry labels and routes:** `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`; `Open parent`; `Runs on its own`.

1. After completion, open Previous agents and any available Show more entries. Inspect the named child, full title tooltip, outcome and history.
2. Open and return through its parent route.

**Required visible outcome:** Completed history is readable and correctly labeled. Merely inspecting it does not restart the child, lose its transcript or navigate to unrelated background work.

**Return and cleanup:** Close only owned agent surfaces while keeping the recorded thread identities for cleanup.

**Sol:** Codex exposes native child IDs, snapshots, waits, close and child fork capabilities. Prove the correct child/parent IDs and live content, with the authorized model recorded.

**Sonnet:** Claude does not declare native child thread IDs, native snapshot/child fork or child approvals, but the app projects child work and stopSubagent can call a live owning query. Verify the observed projected/read-only surface and stop result rather than assuming the Codex model.

**Owning source:** [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/web/src/components/chat/ProviderSubagentBar.tsx::ProviderSubagentBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8428); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13531); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197).

#### T3-C53 · Fork at the selected response and preserve the parent

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-03, COMPOSE-02.

**Entry labels and routes:** `Fork from this response`; `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`; `Provider and model trigger has a dynamic accessible name; record the installed label`.

1. On a completed owned response, use Fork from this response. Inspect the inherited history boundary, project, model and parent link.
2. Send one short FORK_CHECK_T3 prompt asking for the source marker, with no tool or child work.

**Required visible outcome:** The fork starts at the selected boundary, receives the intended context and preserves the parent history/draft. Unsupported child-fork behavior is distinguished from root/turn forks.

**Return and cleanup:** Keep the fork disposable for rewind trials, and record its ID and model turn.

**Sol:** Codex supports root, turn and child forks and rollback. Verify the actual selected boundary and parent isolation, not just a new thread title.

**Sonnet:** Claude supports root/turn forks and rollback via its transcript path, not native child forks. Verify which boundary is available and whether the source history is preserved.

**Owning source:** [`apps/web/src/components/chat/TimelineAssistantRows.tsx::AssistantForkButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineAssistantRows.tsx#L99); [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/web/src/components/chat/ProviderModelPicker.tsx::ProviderModelPicker`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderModelPicker.tsx#L89); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::forkThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8539); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::forkThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13614).

#### T3-C54 · Create a side chat without disturbing main work

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-03, COMPOSE-02.

**Entry labels and routes:** `/model`; `/side`; `/btw`; `/clear`; `Side chat`; `Bring back`; `Delete side chat`; `Close side chat`; `Provider and model trigger has a dynamic accessible name; record the installed label`.

1. Use /side or /btw on the completed audit conversation. Record the side chat ID, model and context.
2. Send one short SIDE_CHECK_T3 prompt and return to the main composer.

**Required visible outcome:** The side chat receives the intended context and renders its reply while the main draft and history remain intact. The authorized provider/model is clear.

**Return and cleanup:** Keep this owned side chat for Bring back and reopen cases; no additional side chat is needed.

**Sol:** Codex supports root, turn and child forks and rollback. Verify the actual selected boundary and parent isolation, not just a new thread title.

**Sonnet:** Claude supports root/turn forks and rollback via its transcript path, not native child forks. Verify which boundary is available and whether the source history is preserved.

**Owning source:** [`apps/web/src/components/chat/useComposerMenuItems.ts::useComposerMenuItems`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L61); [`apps/web/src/components/chat/SideChatPane.tsx::SideChatPane`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SideChatPane.tsx#L36); [`apps/web/src/components/chat/useSideChat.ts::useSideChat`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useSideChat.ts#L26); [`apps/web/src/components/chat/ProviderModelPicker.tsx::ProviderModelPicker`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderModelPicker.tsx#L89); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353).

#### T3-C55 · Bring side-chat context back into an existing draft

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** QUOTE-01, QUOTE-02, COMPOSE-02.

**Entry labels and routes:** `Side chat`; `Bring back`; `Delete side chat`; `Close side chat`; `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Composer accessible name is passed by the owner; record the actual name`.

1. Enter MAIN_DRAFT_C in the main composer. From the completed side chat use Bring back. Inspect the next-message context and editable text.

**Required visible outcome:** The side-chat context is added to the intended main draft without erasing MAIN_DRAFT_C or sending automatically. Its source and scope remain inspectable.

**Return and cleanup:** Remove the brought-back context after recording unless it is intentionally used in the one short recovery turn.

**Sol:** Verify a real Sol turn receives the distinct text, citation and attachment markers applicable to this case once. Model acknowledgement is semantic evidence, not byte-exact transport evidence.

**Sonnet:** Verify a real Sonnet turn receives the corresponding markers once; Claude attachment conversion and history differ from Codex. Do not inherit a Sol delivery result.

**Owning source:** [`apps/web/src/components/chat/SideChatPane.tsx::SideChatPane`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SideChatPane.tsx#L36); [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268); [`apps/web/src/lib/assistantCitationNavigation.ts::assistantCitationNavigation`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/assistantCitationNavigation.ts#L79).

#### T3-C56 · Close and reopen the same side chat with history and draft

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-03, COMPOSE-02, RESTORE-01.

**Entry labels and routes:** `Side chat`; `Bring back`; `Delete side chat`; `Close side chat`; `Reopen Closed Tab`; `Reopen the closed side-chat surface while its thread still exists`; `Source defaults only; inspect installed custom keymap`; `Cmd+F`; `Cmd+S`; `Cmd+Shift+T`; `Cmd+W`; `Cmd+Shift+Enter`; `Alt+ArrowUp`; `Cmd+Enter`.

1. Enter SIDE_DRAFT_C in the existing side chat without sending. Use Close side chat, then the observed Reopen Closed Tab action or default Cmd+Shift+T.
2. Activate the restored view and inspect identity, history, input and model after it settles.

**Required visible outcome:** The same surviving side chat returns with readable history and editable unsent draft. A restored empty tab header alone is not a pass; deleted root threads are outside this closed-surface contract.

**Return and cleanup:** Clear the side draft after capture and close the owned surface again when cleanup begins.

**Sol:** Verify editable draft/history restoration using real navigation or a normal restart after work is quiescent. Codex native snapshot support is not proof of UI restore; live-only pending callbacks do not survive restart.

**Sonnet:** Verify the actual transcript/side-chat and editable draft after restore. Claude does not declare native snapshots or restart-surviving pending requests; record clean recovery or explicit unavailable state, not invented continuity.

**Owning source:** [`apps/web/src/components/chat/SideChatPane.tsx::SideChatPane`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SideChatPane.tsx#L36); [`apps/web/src/components/ReopenClosedViewShortcut.tsx::ReopenClosedViewShortcut`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ReopenClosedViewShortcut.tsx#L62); [`apps/web/src/reopenClosedView.ts::reopenClosedView`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/reopenClosedView.ts#L94); [`packages/shared/src/keybindings.ts::DEFAULT_KEYBINDINGS`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L57); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197); [`apps/web/src/components/settings/KeybindingsSettings.logic.ts::commandLabel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/KeybindingsSettings.logic.ts#L371).

#### T3-C57 · Dismiss only the focused foreground surface

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** SHELL-03, SHELL-04, ACCESS-01.

**Entry labels and routes:** `Side chat`; `Bring back`; `Delete side chat`; `Close side chat`; `Source defaults only; inspect installed custom keymap`; `Cmd+F`; `Cmd+S`; `Cmd+Shift+T`; `Cmd+W`; `Cmd+Shift+Enter`; `Alt+ArrowUp`; `Cmd+Enter`; `Find in thread`; `Previous match`; `Next match`; `Close find`; `Searching…`; `Search failed`; `Retry`; `Comment on selected text`; `Add an optional comment...`; `Cancel`; `Shorten comment`; `Save`; `Close ${mediaLabel} preview`; `Previous media`; `Next media`.

1. With a side/right panel focused, exercise the actual Cmd+W binding and inspect what closes.
2. In separate trials use Escape on an attachment preview, citation editor, Find bar and menu, keeping a main draft present.

**Required visible outcome:** The observed keymap and focus scope agree with the closed/dismissed surface. Main draft/history survive, and return focus is usable. A terminal-focused Cmd+W is a different binding and must not be conflated.

**Return and cleanup:** Reopen any needed owned surface, restore the starting focus and clear exploratory drafts.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/SideChatPane.tsx::SideChatPane`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SideChatPane.tsx#L36); [`packages/shared/src/keybindings.ts::DEFAULT_KEYBINDINGS`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L57); [`apps/web/src/components/chat/ThreadFindBar.tsx::ThreadFindBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadFindBar.tsx#L26); [`apps/web/src/components/chat/AssistantCitationCommentEditor.tsx::AssistantCitationCommentEditor`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L10); [`apps/web/src/components/chat/ExpandedImageDialog.tsx::ExpandedImageDialog`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ExpandedImageDialog.tsx#L80); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/web/src/components/settings/KeybindingsSettings.logic.ts::commandLabel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/KeybindingsSettings.logic.ts#L371).

#### T3-C58 · Cancel Edit from here without changing history

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-05, RESTORE-01.

**Entry labels and routes:** `Edit from here`; `Edit from here?`; `Cancel`; `Revert files too`; `Revert and keep changes`.

1. On the disposable fork, choose Edit from here on a known user message. Inspect the confirmation and select Cancel.

**Required visible outcome:** Cancel leaves fork messages, prompt, attachments and scratch files unchanged. Parent history remains untouched.

**Return and cleanup:** Keep the fork intact for the deliberate rewind trial.

**Sol:** Codex supports root, turn and child forks and rollback. Verify the actual selected boundary and parent isolation, not just a new thread title.

**Sonnet:** Claude supports root/turn forks and rollback via its transcript path, not native child forks. Verify which boundary is available and whether the source history is preserved.

**Owning source:** [`apps/web/src/components/chat/TimelineUserMessageRow.tsx::RevertUserMessageButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L1102); [`apps/web/src/components/chat/ChatOverlayHosts.tsx::RevertConfirmHost`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatOverlayHosts.tsx#L196); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::rollbackThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8463); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::rollbackThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13539).

#### T3-C59 · Rewind the chosen fork while keeping file changes

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-05, QUOTE-03, ATTACH-02.

**Entry labels and routes:** `Edit from here`; `Edit from here?`; `Cancel`; `Revert files too`; `Revert and keep changes`; `Rewind the selected disposable thread at the selected message`; `Restore the editable prompt and attachments after rewind`.

1. On the same disposable fork, select the known user message and choose Revert and keep changes.
2. Inspect the history boundary, restored editable prompt/attachments, current scratch files and the original parent.

**Required visible outcome:** The fork rewinds to the documented point and restores the intended prompt/attachments. Existing file changes remain, and the parent does not rewind.

**Return and cleanup:** Do not resend automatically; retain the disposable fork only until evidence is complete.

**Sol:** Codex supports root, turn and child forks and rollback. Verify the actual selected boundary and parent isolation, not just a new thread title.

**Sonnet:** Claude supports root/turn forks and rollback via its transcript path, not native child forks. Verify which boundary is available and whether the source history is preserved.

**Owning source:** [`apps/web/src/components/chat/TimelineUserMessageRow.tsx::RevertUserMessageButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L1102); [`apps/web/src/components/chat/ChatOverlayHosts.tsx::RevertConfirmHost`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatOverlayHosts.tsx#L196); [`apps/web/src/components/chat/chatViewSend.ts::rewindToMessage`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/chatViewSend.ts#L154); [`apps/web/src/components/chat/chatViewSend.ts::restoreComposerDraft`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/chatViewSend.ts#L257); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::rollbackThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8463); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::rollbackThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13539).

#### T3-C60 · Inspect the file-revert branch without risking unrelated work

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-05, FILES-02.

**Entry labels and routes:** `Edit from here?`; `Cancel`; `Revert files too`; `Revert and keep changes`; `Rewind the selected disposable thread at the selected message`.

1. Inspect Revert files too on an owned disposable checkpoint and record availability and scope.
2. Execute only when the parent has established that every affected file/checkpoint is inside the disposable scratch fixture; otherwise record the branch as not-reached.

**Required visible outcome:** The displayed affected scope agrees with the actual restored files, or a disabled/unavailable state explains why the action cannot run. No unrelated project changes are part of this case.

**Return and cleanup:** Preserve evidence of the scratch state before and after, then remove only owned fixtures during cleanup.

**Sol:** Codex supports root, turn and child forks and rollback. Verify the actual selected boundary and parent isolation, not just a new thread title.

**Sonnet:** Claude supports root/turn forks and rollback via its transcript path, not native child forks. Verify which boundary is available and whether the source history is preserved.

**Owning source:** [`apps/web/src/components/chat/ChatOverlayHosts.tsx::RevertConfirmHost`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatOverlayHosts.tsx#L196); [`apps/web/src/components/chat/chatViewSend.ts::rewindToMessage`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/chatViewSend.ts#L154); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::rollbackThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8463); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::rollbackThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13539).

#### T3-C61 · Start or cancel a fresh session while retaining visible history

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** RESTORE-01, COMPOSE-02.

**Entry labels and routes:** `/model`; `/side`; `/btw`; `/clear`; `Your next message starts a fresh session`; `Keep the current session`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`.

1. Use /clear on a disposable owned conversation, inspect the fresh-session banner and choose Keep the current session.
2. If needed for coverage, repeat and intentionally send one short marker in the fresh session, within the recorded task budget.

**Required visible outcome:** Cancellation keeps the current context. A deliberately fresh send follows the displayed promise that earlier messages remain visible but are not supplied to the new session; the model turn is counted separately.

**Return and cleanup:** Leave no accidental pending fresh-session state on the user original thread.

**Sol:** Verify editable draft/history restoration using real navigation or a normal restart after work is quiescent. Codex native snapshot support is not proof of UI restore; live-only pending callbacks do not survive restart.

**Sonnet:** Verify the actual transcript/side-chat and editable draft after restore. Claude does not declare native snapshots or restart-surviving pending requests; record clean recovery or explicit unavailable state, not invented continuity.

**Owning source:** [`apps/web/src/components/chat/useComposerMenuItems.ts::useComposerMenuItems`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L61); [`apps/web/src/components/chat/ComposerFreshSession.tsx::freshSessionBannerItem`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerFreshSession.tsx#L6); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7809); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::startTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12353); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197); [`apps/web/src/components/chat/useComposerSend.ts::useComposerSend`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L268).

#### T3-C62 · Restore real history and structured drafts after a normal restart

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** RESTORE-01, COMPOSE-02, QUOTE-03, ATTACH-02.

**Entry labels and routes:** `Composer accessible name is passed by the owner; record the actual name`; `View cited assistant text: ${label}`; `Add comment to citation`; `Edit citation comment`; `Preview ${image.name}`; `Draft attachment may not persist`; `Retry upload for ${name}`; `Remove ${name}`; `Side chat`; `Bring back`; `Delete side chat`; `Close side chat`; `Reopen the closed side-chat surface while its thread still exists`.

1. After every owned task and request is settled, record main/side drafts, citations, attachments, model and visible reading position. The parent performs a normal reload or app restart when unrelated work is safe.
2. Reopen the owned views and inspect actual content, input editability and context.

**Required visible outcome:** The exercised persisted state returns coherently, or a precise recovery failure is captured. Source snapshot support is not proof, and pending provider callbacks are not assumed to survive restart.

**Return and cleanup:** Restore the original app focus and preferences; leave no unsent audit drafts or attachment uploads after evidence is retained.

**Sol:** Verify editable draft/history restoration using real navigation or a normal restart after work is quiescent. Codex native snapshot support is not proof of UI restore; live-only pending callbacks do not survive restart.

**Sonnet:** Verify the actual transcript/side-chat and editable draft after restore. Claude does not declare native snapshots or restart-surviving pending requests; record clean recovery or explicit unavailable state, not invented continuity.

**Owning source:** [`apps/web/src/components/ComposerPromptEditorTiptap.tsx::ComposerPromptEditorTiptap`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L760); [`apps/web/src/components/chat/AssistantCitationChip.tsx::AssistantCitationChip`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L179); [`apps/web/src/components/chat/ComposerAttachmentStrip.tsx::ComposerAttachmentMediaRow`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L136); [`apps/web/src/components/chat/SideChatPane.tsx::SideChatPane`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SideChatPane.tsx#L36); [`apps/web/src/reopenClosedView.ts::reopenClosedView`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/reopenClosedView.ts#L94); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L8428); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::readThreadSnapshot`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13531); [`apps/web/src/composerDraftStore.ts::composerDraftStore`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2673); [`apps/web/src/composerDraftStore.ts::partializeComposerDraftStoreState`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2246); [`apps/web/src/composerDraftStore.ts::hydrateImagesFromPersisted`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2557).

#### T3-C63 · Navigate among main, child, fork and side chat without crossing state

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-01, AGENT-03, COMPOSE-02, RESTORE-01.

**Entry labels and routes:** `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`; `Open parent`; `Runs on its own`; `Side chat`; `Bring back`; `Delete side chat`; `Close side chat`; `Source defaults only; inspect installed custom keymap`; `Cmd+F`; `Cmd+S`; `Cmd+Shift+T`; `Cmd+W`; `Cmd+Shift+Enter`; `Alt+ArrowUp`; `Cmd+Enter`.

1. Use explicit parent/fork/side links and the installed back/forward navigation through the known owned surfaces. At each stop inspect title, ID, model, draft and reading position.

**Required visible outcome:** Each transition reaches the intended conversation and keeps its own state. Returning does not exchange drafts, show a different agent history or unexpectedly start work.

**Return and cleanup:** End on the original user surface after all audit-owned views are closed.

**Sol:** Verify editable draft/history restoration using real navigation or a normal restart after work is quiescent. Codex native snapshot support is not proof of UI restore; live-only pending callbacks do not survive restart.

**Sonnet:** Verify the actual transcript/side-chat and editable draft after restore. Claude does not declare native snapshots or restart-surviving pending requests; record clean recovery or explicit unavailable state, not invented continuity.

**Owning source:** [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/web/src/components/chat/ProviderSubagentBar.tsx::ProviderSubagentBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14); [`apps/web/src/components/chat/SideChatPane.tsx::SideChatPane`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SideChatPane.tsx#L36); [`packages/shared/src/keybindings.ts::DEFAULT_KEYBINDINGS`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L57); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7698); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::resumeThread`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13197); [`apps/web/src/navigationHistory.ts::navigateHistory`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/navigationHistory.ts#L14).

#### T3-C64 · Read and export an actual proposed plan

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** READ-05, FILES-05.

**Entry labels and routes:** `Plan actions`; `Copy to clipboard`; `Download as markdown`; `Save to workspace`; `Find in thread`; `Previous match`; `Next match`; `Close find`; `Searching…`; `Search failed`; `Retry`.

1. If a real proposed-plan card is emitted, expand/collapse it, search within its content, then use Plan actions to copy and download it.
2. Use Save to workspace only for an owned scratch destination and inspect the resulting file.

**Required visible outcome:** Plan content stays readable and exported content matches the displayed plan. A generic Markdown answer is not substituted for the real plan-card route.

**Return and cleanup:** Remove only audit plan exports after retaining evidence; leave the branch not-reached if no real card was emitted.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/ProposedPlanCard.tsx::ProposedPlanCard`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProposedPlanCard.tsx#L41); [`apps/web/src/components/chat/ThreadFindBar.tsx::ThreadFindBar`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadFindBar.tsx#L26); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C65 · Record conditional voice and compaction states without inventing availability

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** COMPOSE-01, ACCESS-01.

**Entry labels and routes:** `Cancel voice input`; `Finish voice input`; `Submit message`; `Queue message`; `Steer message`; `Update queued message`; `Resume thread`; `Compact and send`.

1. Inspect voice availability and its actual setup/disabled reason. If already configured, start and cancel an owned short input, then inspect the draft.
2. If Compact and send appears during real bounded work, inspect its effect on the intended draft and turn; do not manufacture token exhaustion to force it.

**Required visible outcome:** Cancel voice input stops capture and preserves intended draft content. Compaction, if reached, sends the intended prompt once with truthful state. Unavailable and not-reached branches remain distinct from passes.

**Return and cleanup:** End any owned recording and restore the starting composer state.

**Sol:** Verify the selected Sol thread identity and actual visible action; a shared widget does not establish provider delivery.

**Sonnet:** Repeat the shared interaction in the Sonnet lane and record its identity. A Sol pass is not a Sonnet pass.

**Owning source:** [`apps/web/src/components/chat/ComposerSpeechButton.tsx::ComposerSpeechButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerSpeechButton.tsx#L91); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::ComposerPrimaryActions`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L222); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

#### T3-C66 · Finish with no owned work or accidental queue left behind

**State:** phase C; Sol `not-run`; Sonnet `not-run`. **Ferrite journeys:** AGENT-02, AGENT-03, COMPOSE-04, RESTORE-01.

**Entry labels and routes:** `${items.length} queued messages`; `Queued message ${position + 1} of ${items.length}, not sent`; `Send now`; `Editing in the composer`; `Cancel editing queued message`; `Edit queued message`; `Remove queued message`; `Remove from queue`; `Show all ${items.length} queued`; `Show fewer`; `Stop generation`; `Stopping`; `Related threads`; `Previous agents`; `Subagent`; `Parent agent`; `Fork`; `Parent thread`; `Stop subagent ${title}`; `Stop subagent`; `Lineage`; `Lineage · ${runningCount} running`; `More thread actions`; `Side chats`; `Side chat`; `Bring back`; `Delete side chat`; `Close side chat`; `Details`; `Background work`; `Waiting`; `Running`; `Open thread`.

1. Enumerate all audit root/fork/side/child/grandchild IDs and model turns. Remove every unused queued item, resolve or cancel requests, stop owned running work and verify bounded commands are no longer alive.
2. Close owned views and browser tabs, remove audit stash/drafts/uploads, and restore original model/permission/panel/focus settings.

**Required visible outcome:** Every started audit task is accounted for as completed or stopped. No owned command, recording, queued turn, pending request or child is left running. Only audit-owned objects are archived/deleted under the parent authorized cleanup scope.

**Return and cleanup:** Retain screenshots, result records and source links. Report any remaining owned object explicitly rather than hiding a cleanup failure.

**Sol:** Verify editable draft/history restoration using real navigation or a normal restart after work is quiescent. Codex native snapshot support is not proof of UI restore; live-only pending callbacks do not survive restart.

**Sonnet:** Verify the actual transcript/side-chat and editable draft after restore. Claude does not declare native snapshots or restart-surviving pending requests; record clean recovery or explicit unavailable state, not invented continuity.

**Owning source:** [`apps/web/src/components/chat/QueuedRunsControl.tsx::QueuedRunsControl`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L77); [`apps/web/src/components/chat/ComposerPrimaryActions.tsx::StopGenerationButton`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L132); [`apps/web/src/components/chat/ThreadRelationshipsControl.tsx::ThreadRelationshipsPanel`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L424); [`apps/web/src/components/chat/SideChatPane.tsx::SideChatPane`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SideChatPane.tsx#L36); [`apps/web/src/components/chat/BackgroundWorkDetails.tsx::BackgroundWorkDetails`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L21); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255); [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::interruptTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L7920); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::interruptTurn`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L12722); [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::stopSubagent`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L13419); [`apps/web/src/lib/composerDraftUploads.ts::releaseComposerDraftUploads`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/composerDraftUploads.ts#L7).

## Where things live

Each case above links its frontend owner and relevant provider boundary. The JSON contains the same paths, symbols and line anchors, plus exact result slots. These provider differences must remain separate in the live report.

| Path being exercised | Sol / Codex | Sonnet / Claude |
| --- | --- |
| Streaming | Assistant, reasoning, tool and plan stream capabilities. Require live increments for the actual type exercised. | Assistant and reasoning streams; generic tool/plan stream capabilities are false. `tailClaudeTaskOutput` can still deliver live background command output. Observe the route. |
| Steering | Active `turn/steer`, interruption and queueing. | Active steering can interrupt tools and background foreground subagents. Observe ownership after steering. |
| Permissions | Runtime mode changes are supported within the native session; Supervised maps to approval-required execution. | Runtime mode changes are not supported within the native session. Set Supervised before starting and verify effective behavior. |
| Questions | `requestUserInput` and answer mapping. | `AskUserQuestion` through `canUseToolEffect`, or actual elicitation through `onElicitationEffect`. |
| Children | Native IDs, snapshots, wait/close, child forks and child approvals. | Native child-ID/snapshot/fork/approval capabilities differ; app-projected work and live-query `stopSubagent` still exist. |
| Fork/restore | Root/turn/child forks, rollback and native snapshots. | Root/turn forks and transcript-based restoration; no native child fork or native snapshot capability. |
| Pending callbacks after restart | Not declared to survive restart. | Not declared to survive restart. |

**Sol provider boundary:** [`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts::CodexProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L255).

**Sonnet provider boundary:** [`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts::ClaudeProviderCapabilitiesV2`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts#L489).

**Claude live output boundary:** [`apps/server/src/orchestration-v2/Adapters/ClaudeTaskOutputTail.ts::tailClaudeTaskOutput`](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/Adapters/ClaudeTaskOutputTail.ts#L61).

Source boundaries are 8000 citation characters, 8000 comment characters, three initially visible queue rows and 100 attachments per message. Those constants do not certify their runtime boundary behavior. The checklist deliberately does not manufacture 100 uploads or token exhaustion.

## Gotchas

- Capture actual selection and input focus. A wrapper rectangle or toolbar appearance does not prove text entered the nested field.
- A model acknowledgement can show semantic receipt of a file/image/citation marker. It does not prove byte-exact transport fidelity.
- Do not replace a real structured question, native child record, proposed-plan card or approval with visually similar prose and call the case tested.
- Read-only native subagent work and editable child threads have different contracts. Report the route used and the correct parent identity.
- Closed side-chat reopening requires the underlying thread to survive. It is not deletion recovery for arbitrary root threads.
- If the task completes before a transient state is reached, keep that branch `not-reached` or deliberately record a separately bounded continuation. Do not leave model work running indefinitely.
- Complete C66 even if earlier cases fail. Account for every task, queued prompt, pending request, upload, recording and descendant; restore the original user surface and settings.

This document is ready to drive live verification. The native app execution and evidence reconciliation remain the parent task; no complete T3 journey is certified by this checklist.
