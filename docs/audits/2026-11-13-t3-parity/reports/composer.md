# Composer audit

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants a detailed, source-backed account of Ferrite's broken workflows, missing T3 capabilities and weak interactions. This slice covers drafting, sending, queuing, steering, stopping, attachment intake, commands, mentions, model/effort controls, context, checkout selection, editing and retry. It distinguishes accidental failures from deliberate product differences.

## Overview

Ferrite has a substantial working composer implementation. The main risks are admission-time prompt loss, the lack of persistence for a new unsent pane, and local attachments presented as usable in remote threads. Queue editing and stopping also differ materially from T3. The newer T3 composer is broader than the historical reference: it adds structured context, entity mentions, stash, rich Markdown, voice and multi-model first sends.

There are 15 findings in this slice. The two conditional prompt-loss findings are P1 under the parent's severity normalization: new-pane loss concerns editor state before Submit, while early send refusal loses a prompt after Submit. Neither establishes loss of committed repository files or general transcript corruption. Intentional differences and optional P3 additions are called out separately. All findings are source-backed rather than runtime reproductions.

## Method and limits

- Ferrite source baseline: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- T3 source baseline: `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, inspected at `/tmp/ferrite-t3-audit-source-20261113` from `pingdotgg/t3code`.
- Throughput checkpoint: n/a, read-only investigation.
- Read the audit brief, Codex pstack adapter, investigation instructions and unslop writing instructions. Followed event handlers through draft state, wire serialization, admission, provider input and recovery.
- Used `rg`, numbered source reads and repository-wide searches for alternate stash, voice, upload, mention and rich-editor paths. A negative search alone is not treated as a runtime proof. The findings also cite the state models and exhaustive routing functions that constrain the behavior.
- Did not launch apps, drive the live UI, run providers, edit application code, change settings, run tests, commit or open a PR. No measured timing or performance claim is made. Existing tests were inspected as coverage evidence only.
- T3 mobile-only behavior is recorded separately below and is not a native-desktop defect by default. Provider catalogs, permissions, goals and service-tier parity belong primarily to the providers/settings slice.

## Key concepts

A Ferrite new pane has a `DraftId` and `DraftBinding`, but no `ThreadId` or provider session. A thread draft has one persisted string per `ThreadId`. Its `Composer` separately holds a `Line`, files, reply cards, review comments, browser annotations and file/command/skill token metadata. Sending flattens these into a string. Provider adapters derive typed file/image/skill input from that string.

A Ferrite queue is provider-owned, with locally held inputs during startup and correlation metadata for acknowledged provider inputs. Its `queue` name does not mean the user selected an after-turn queue. With an active Codex turn it sends a steer. `Edit` means cancel/take back, and `Stop` means stop the current turn while letting pending work proceed.

T3 keeps structured drafts and context records, separately identifies new-draft sessions and queued-edit drafts, and carries explicit dispatch intent. An attachment belongs to an environment and may have preparation/upload/failure state. These distinctions explain many of the behavior differences below.

## Traced flows

| Flow | Ferrite trace and result | T3 comparison |
| --- | --- | --- |
| Edit existing-thread draft | `composer_edited` calls `keep_draft`, which serializes `prompt()` and writes after one second. Restore calls `set`. See `crates/ferrite-cockpit/src/cockpit.rs:2204-2228` and `crates/ferrite-cockpit/src/cockpit/drafts.rs:7-64`. | Structured draft persistence includes text, context and draft sessions. [apps/web/src/composerDraftStore.ts:2263-2371](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2263). |
| New draft first send | Resolve provider/project/checkout, bootstrap, then adopt the returned thread. Failed bootstrap leaves text intact; edits during startup are protected by comparing the sent text. See `crates/ferrite-cockpit/src/cockpit.rs:7854-7932`. | New draft promotion and failed-send restoration are explicit. [apps/web/src/components/chat/useComposerSend.ts:1395-1453](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1395). |
| Idle send | Take and trim composer, then void `send`. Normal delivery failure recovers text, but early root/start refusal does not. See `crates/ferrite-cockpit/src/cockpit.rs:5552-5596`, `crates/ferrite-core/src/engine.rs:2537-2578`. | Failed send restores its snapshot or keeps a copy action when newer typing exists. [apps/web/src/components/chat/useComposerSend.ts:1404-1453](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1404). |
| Running send | Busy input calls `queue`; Codex chooses steer when current_turn exists; Claude sends asynchronous input. See `crates/ferrite-core/src/providers/codex.rs:604-623`, `crates/ferrite-core/src/providers/claude.rs:683-699`. | User/default dispatch choice and modifier override. [apps/web/src/components/chat/ComposerPrimaryActions.tsx:250-265](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L250). |
| Queue actions | Stable row IDs reach send-now, edit and remove. Edit cancels and prepends to the draft; send-now can steer or interrupt depending on delivery/provider state. See `crates/ferrite-cockpit/src/cockpit.rs:8098-8123`, `crates/ferrite-core/src/engine.rs:3883-3965`. | Save/cancel edit has a separate draft target and preserves original queue identity. [apps/web/src/components/chat/useQueuedRunEdit.ts:194-219,322-380](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useQueuedRunEdit.ts#L194). |
| Stop | Button targets the selected main thread and preserves typed text. Core interrupt allows held work to proceed. Escape also dismisses menus and scopes subagent stop separately. See `crates/ferrite-cockpit/src/cockpit.rs:5638-5703,8269-8284`, `crates/ferrite-core/src/engine.rs:2660-2701`. | Queue hold and explicit Resume. [apps/web/src/components/chat/useComposerResume.ts:52-55,88-166](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerResume.ts#L52). |
| Add file/image | Picker, clipboard and pane-wide native drop all feed `add_files`; a clipboard image spills to a file; shelf preview/removal is available. See `crates/ferrite-pane/src/composer.rs:572-619,2104-2125`, `crates/ferrite-pane/src/prompt_drop.rs:11-45`, `crates/ferrite-cockpit/src/cockpit.rs:8525-8541`. | Prepared attachments are uploaded to an environment, with failures and progress. [apps/web/src/lib/attachmentUploadQueue.ts:135-161,270-301](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135). |
| Commands/skills | Cursor-driven `/`, `$`, `@` menu; provider command discovery and loading/error rows; local import/provider/model/effort actions; picked/typed skill tokens become provider skill input. See `crates/ferrite-cockpit/src/cockpit.rs:6268-6480`, `crates/ferrite-core/src/providers/codex/wire.rs:1250-1274`. | Slash position rules, provider commands, optional slash-menu skills and separate skill trigger. [apps/web/src/components/chat/useComposerMenuItems.ts:265-321](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L265). |
| Model/effort | Single model picker searches provider catalogs, offers favorites and reasoning choices, and validates effort against the selected model. See `crates/ferrite-cockpit/src/cockpit/model_picker.rs:397-472,697-769,1241-1313`, `crates/ferrite-core/src/draft.rs:64-79`. | Descriptor-driven options and multi-model selection, with provider-specific capabilities. [apps/web/src/components/chat/ComposerModelEffort.tsx:39-46,99-131](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerModelEffort.tsx#L39). |
| Checkout selection | New drafts support current checkout, existing branch/worktree, new branch and new isolated worktree; binding resolves before first send. See `crates/ferrite-core/src/draft.rs:15-35`, `crates/ferrite-cockpit/src/cockpit.rs:7648-7722,7854-7877`. | Supports local/worktree launch, Git branch selection and multi-model bootstrap. [apps/web/src/components/chat/composerSend.logic.ts:39-59](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/composerSend.logic.ts#L39). |
| Recall/edit/retry | Visual-row navigation precedes prompt recall; recalled drafts preserve attachment serialization. Edit/rewind is capability-gated and blocked while busy/queued/waiting. Failed-turn retry recovers the last prompt including attachments. See `crates/ferrite-pane/src/composer.rs:792-821`, `crates/ferrite-cockpit/src/cockpit.rs:6617-6671`, `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:25-46,169-190,283-311`. | Text-only prompt history, capability-gated Edit from here, separate file-revert policy and protected newer draft. [docs/user/composer.md:148-186](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L148). |

## Source map

| Responsibility | Ferrite | T3 |
| --- | --- | --- |
| Editor and tokens | `crates/ferrite-pane/src/composer.rs:139-220,494-619,1142-1310`; `crates/ferrite-pane/src/line.rs:13-42` | [apps/web/src/components/ComposerPromptEditorTiptap.tsx:810-870](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L810) |
| Composer presentation and queue | `crates/ferrite-pane/src/pane/composer.rs:412-708,781-950`; `crates/ferrite-cockpit/src/cockpit.rs:8098-8301` | [apps/web/src/components/chat/ChatComposer.tsx:453-615](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatComposer.tsx#L453); [apps/web/src/components/chat/ComposerPrimaryActions.tsx:250-265](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L250) |
| Durability and launch | `crates/ferrite-core/src/drafts.rs:12-58`; `crates/ferrite-core/src/draft.rs:15-99`; `crates/ferrite-cockpit/src/cockpit/drafts.rs:28-64` | [apps/web/src/composerDraftStore.ts:2263-2371](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2263) |
| Admission and queue | `crates/ferrite-core/src/engine.rs:2537-2701,3706-3974` | [apps/web/src/components/chat/useComposerSend.ts:1110-1130,1404-1453](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1110); [apps/web/src/components/chat/useQueuedRunEdit.ts:162-219](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useQueuedRunEdit.ts#L162) |
| Files and context | `crates/ferrite-core/src/prompt_files.rs:15-126`; `crates/ferrite-pane/src/attachments.rs:36-127`; provider wire modules | [apps/web/src/components/chat/useComposerAttachmentIntake.ts:247-284,371-445](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L247); [apps/web/src/lib/attachmentUploadQueue.ts:135-161](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135) |
| Menus and models | `crates/ferrite-cockpit/src/cockpit.rs:6268-6595`; `crates/ferrite-cockpit/src/cockpit/model_picker.rs:397-472` | [apps/web/src/components/chat/useComposerMenuItems.ts:60-180](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L60); [apps/web/src/components/chat/ComposerModelEffort.tsx:99-131](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerModelEffort.tsx#L99) |

## Detailed findings

### composer-01: An idle send can clear the only draft before admission fails

P1 | defect | source-proven

An unsent prompt and its staged context can disappear when the user sends to an unavailable workspace or a session that cannot start. This is a conditional loss after accepting Submit, not evidence of general transcript loss.

Trigger: In an existing idle thread, type a prompt and make the workspace unavailable, or make provider startup fail, then send. A server command refusal is another path requiring runtime confirmation.

Expected: Keep the complete draft until admission is acknowledged, or restore it after a definite refusal without overwriting newer typing.

Actual: The UI calls Composer.take before send. Engine.send returns after a vanished-root notice or start failure without putting text in the recovered queue. The void server adapter logs command refusal rather than restoring the draft.

Cause: Recovery is attached to deliver errors, but early admission failures have no text-bearing return or recovery event. The UI cannot distinguish an accepted send from a refused one.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:5552-5554,5585-5596`.
- `crates/ferrite-pane/src/composer.rs:516-527`.
- `crates/ferrite-core/src/engine.rs:2537-2578`.
- `crates/ferrite-core/src/engine.rs:989-993`.
- `crates/ferrite-core/src/cockpit.rs:1747-1758,2853-2859`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/useComposerSend.ts:1404-1453](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1404).

Recommendation: Make send admission return an explicit outcome or recover the full snapshot on every definite refusal. Preserve uncertain-send status separately to avoid accidental duplicate sends.

Observable acceptance check: Send a unique multiline prompt with a file and quote after forcing each early refusal. The prompt and context remain editable, the refusal is visible, no user message is falsely shown as delivered, and a later retry sends exactly once. Type a second draft during failure and verify both remain recoverable.

### composer-02: A new unsent pane has no durable draft record

P1 | defect | source-proven

A first prompt composed before a thread exists is lost when the app closes. Existing-thread drafts persist, making the difference hard to anticipate. This is loss of unsent editor state before the user accepts Submit, not loss of an accepted prompt or transcript.

Trigger: Open a new draft pane, type text and add attachments without sending, quit normally, and reopen.

Expected: Restore the new draft with its project, checkout, provider, effort, text and attachments, or explicitly protect it before closing.

Actual: composer_edited only calls keep_draft when pane.thread returns Some. Draft panes return None. The draft store only has ThreadId keys, and the new-pane binding lives in memory.

Cause: The domain separates DraftId from ThreadId correctly, but persistence only models the latter. A deferred first send has no durable owner.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:2204-2228`.
- `crates/ferrite-core/src/roster.rs:19-48`.
- `crates/ferrite-core/src/drafts.rs:12-17,34-58`.
- `crates/ferrite-core/src/draft.rs:27-35`.
- `crates/ferrite-cockpit/src/cockpit/drafts.rs:28-64`.

T3 evidence at the pinned commit:

- [apps/web/src/composerDraftStore.ts:2263-2371](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2263).
- [apps/web/src/composerDraftStore.ts:2580-2641,4420-4437](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2580).

Recommendation: Persist DraftId-owned records separately from thread drafts, including the launch binding, and migrate ownership atomically on first send.

Observable acceptance check: Create two unsent new drafts with different models, branches and attachments. Quit normally and reopen. Both return unchanged; sending one promotes only that draft and does not duplicate it on a second restart.

### composer-03: Ordinary local attachments are not transferred to a remote machine

P1 | defect | source-proven

A local file or pasted screenshot can look attached in a remote thread while the agent receives only a client-local path that does not exist on its machine.

Trigger: Connect a remote machine, attach a file that exists only on the client through the picker, clipboard or native drop, then send.

Expected: Upload the bytes to the selected machine and send its attachment reference, or block the unsupported action with a clear explanation.

Actual: The picker and paste/drop paths collect local PathBuf values. prompt_files encodes those as absolute @ references. Send serializes the text. Codex only emits typed file/image items when that path exists on the provider host; Claude silently skips a failed open and retains the unusable path text.

Cause: Ordinary attachments have no environment-owned upload step. The separate browser-capture upload machinery does not handle these composer file paths.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:8525-8541`.
- `crates/ferrite-pane/src/composer.rs:572-618`.
- `crates/ferrite-pane/src/prompt_drop.rs:36-45`.
- `crates/ferrite-core/src/prompt_files.rs:15-24`.
- `crates/ferrite-core/src/cockpit.rs:1747-1758`.
- `crates/ferrite-core/src/providers/codex/wire.rs:1276-1293`.
- `crates/ferrite-core/src/providers/claude/wire.rs:28-55`.

T3 evidence at the pinned commit:

- [apps/web/src/lib/attachmentUploadQueue.ts:135-161,270-301](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135).
- [apps/web/src/components/chat/useComposerSend.ts:1110-1114](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1110).

Recommendation: Represent ordinary attachment ownership and transfer state explicitly. Stage bytes on the destination before admitting a send, retain progress/errors, and never reinterpret client absolute paths as remote files.

Observable acceptance check: Attach a client-only PNG and text file to a remote thread using all three intake methods. The agent can read their exact contents; upload failure retains the draft and offers retry/removal. A coincidentally identical remote path must not substitute different bytes.

### composer-04: Queued Edit removes the message and merges it into unrelated draft text

P2 | partial-capability | source-proven

Editing a queued message changes queue membership immediately and combines it with a separate draft. There is no save-in-place or cancel that restores both original pieces of work.

Trigger: Queue A and B, type an unrelated draft C, then click Edit on A.

Expected: An edit session preserves queue order and draft C until Save or Cancel. Save changes A in place; Cancel leaves A and C untouched.

Actual: Edit calls cancel_queue with Restore prepend=true. Once cancellation arrives, the UI combines held text and current draft with two newlines. A later Send creates a new submission instead of editing A at its original position.

Cause: The operation is a take-back action labeled Edit. There is no separate edit snapshot, queue identity-preserving update, save or cancellation state.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:8098-8123`.
- `crates/ferrite-core/src/cockpit.rs:2388-2390`.
- `crates/ferrite-core/src/engine.rs:3883-3886,3925-3965`.
- `crates/ferrite-cockpit/src/cockpit.rs:2337-2358`.
- `crates/ferrite-pane/src/pane/composer.rs:925-928`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/useQueuedRunEdit.ts:162-184,194-219,322-380](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useQueuedRunEdit.ts#L162).

Recommendation: Either rename this operation Take back and disclose its merge behavior, or add transactional queued editing with independent draft storage and a provider/server edit operation.

Observable acceptance check: With queued A/B and draft C, edit A and cancel: A/B order and C remain unchanged. Repeat and save A2: only A changes, C is restored, and no extra turn is started. Cover A starting while its edit is open.

### composer-05: The composer cannot choose queue versus steer per message

P2 | missing-capability | source-proven

Users cannot deliberately defer a follow-up until the current turn finishes. A message intended as later work can alter the running turn.

Trigger: While an agent works, type a follow-up that must wait for completion and look for a queue action or alternate-send shortcut.

Expected: Offer an explicit Queue or Steer choice, capability-gate steering, and make the active choice visible before sending.

Actual: The send affordance is always described as steering while running. submit routes busy input to queue, whose Codex adapter selects steer whenever current_turn exists. There is no submission-intent parameter or modifier branch in this path.

Cause: Provider-native admission determines timing. Ferrite has no per-message user intent distinguishing after-turn work from steering.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:5585-5596,8243-8264`.
- `crates/ferrite-core/src/engine.rs:3706-3748`.
- `crates/ferrite-core/src/providers/codex.rs:604-621`.
- `crates/ferrite-core/src/providers/claude.rs:683-699`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/ComposerPrimaryActions.tsx:250-265,393-398](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L250).
- [packages/client-runtime/src/state/composerDispatch.ts:1-51](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/client-runtime/src/state/composerDispatch.ts#L1).
- [docs/user/composer.md:63-91](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L63).

Recommendation: Add an explicit dispatch intent through UI, protocol and provider adapters. Keep a local/default behavior preference with a one-shot alternate action.

Observable acceptance check: In both providers, Queue leaves the running turn unmodified and starts only after it ends; Steer reaches the running turn. Pointer tooltip, modifier-click, keyboard and accessibility label describe the same action. Unsupported steer never appears available.

### composer-06: Stop deliberately allows queued work to continue

P2 | intentional-difference | source-proven

A user expecting Stop to stop automation can see queued work start immediately. This is intentional current code behavior, not an accidental missing interrupt.

Trigger: Have a running turn plus pending input and click Stop or press Escape.

Expected: For T3 parity, stop the current turn and hold queued work until explicit Resume. If Ferrite keeps its existing policy, distinguish Stop current turn from Stop and hold queue visibly.

Actual: Engine.interrupt explicitly treats Escape with held prompts as stop then do these. Claude runs its surviving queue; Codex marks pending input for resend. T3 exposes held-queue Resume.

Cause: The queue continues under provider ownership, and interrupt establishes a resend policy instead of a paused-queue state.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:8269-8284`.
- `crates/ferrite-core/src/engine.rs:2660-2701`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/useComposerResume.ts:52-55,88-166](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerResume.ts#L52).
- [docs/user/composer.md:246-271](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L246).

Recommendation: Make stopping semantics an explicit product decision. Prefer a hold state that survives restart, with Resume and separately named Send now behavior.

Observable acceptance check: With pending A/B, Stop visibly acknowledges interruption and neither A nor B runs until Resume. If retaining current semantics, the label and confirmation-free explanatory text must clearly state that queued work will continue.

### composer-07: Attachment readiness and native-image fallbacks are invisible

P2 | partial-capability | source-proven

A chip can imply that a model received an image even when it got only a path. Missing or oversized files have no composer warning. HEIC/HEIF is not a native image format in the provider attachment classifier.

Trigger: Attach a deleted file, a Claude image over 5 MiB, images beyond the 20 MiB aggregate inline budget, or a HEIC image; then send.

Expected: Validate or prepare attachments before send and show whether the model receives image bytes, a readable file reference, or an unusable attachment. Convert supported camera formats when needed.

Actual: add_files only deduplicates paths. Claude silently continues on failed reads and size limits. prompt_files only recognizes PNG/JPEG/GIF/WebP as native images. T3 prepares/compresses images and reports unreadable/too-large failures.

Cause: Composer attachment state is a path vector, with no preparation, ready, fallback or failed state. Adapter fallback is not returned to the UI.

Ferrite evidence:

- `crates/ferrite-pane/src/composer.rs:139-158,572-582`.
- `crates/ferrite-core/src/prompt_files.rs:96-105`.
- `crates/ferrite-core/src/providers/claude/wire.rs:23-55`.
- `crates/ferrite-core/src/providers/codex/wire.rs:1276-1293`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/useComposerAttachmentIntake.ts:247-284](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L247).
- [apps/web/src/lib/attachmentUploadQueue.ts:135-161](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135).
- [docs/user/composer.md:38-56](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L38).

Recommendation: Add provider-aware validation and explicit attachment state, including a visible file-reference fallback. Prepare images off the UI thread, and retain original bytes where needed.

Observable acceptance check: Each boundary case produces either a ready native image or a visible explanation before Send. Deleted/unreadable files block or require removal. HEIC is converted or clearly labeled as a file. The agent sees the expected image bytes for accepted inputs.

### composer-08: Large pastes stay inline with no composer length guard

P2 | missing-capability | source-proven

Large logs or documents fill the prompt directly and can hit provider or transport limits without an early actionable message. No performance regression is claimed without measurement.

Trigger: Paste a large log, then paste text large enough to exceed the selected provider input limit.

Expected: Offer an automatic text-file attachment for large pastes, an explicit keep-inline override, and a length error that preserves the draft.

Actual: paste_item normalizes line endings and inserts all text. The inspected submit path trims and sends without a length validator. T3 has paste disposition, attachment limits, preparation errors and a dedicated prompt-length alert.

Cause: The native editor models the paste as ordinary String replacement; there is no large-paste policy or length-validation stage before admission.

Ferrite evidence:

- `crates/ferrite-pane/src/composer.rs:594-619`.
- `crates/ferrite-cockpit/src/cockpit.rs:5552-5596`.
- `crates/ferrite-pane/src/line.rs:13-32`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/useComposerAttachmentIntake.ts:371-445](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L371).
- [apps/web/src/components/chat/ComposerPromptLengthValidation.tsx:1-13](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPromptLengthValidation.tsx#L1).
- [docs/user/composer.md:6-13](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L6).

Recommendation: Add a provider-aware send limit and optional large-paste-to-file path, retaining an editable override. Do not copy T3 numeric limits blindly.

Observable acceptance check: A large paste becomes a named readable text attachment with feedback; the override preserves inline text. Oversize sends do not clear content and explain how to proceed. Test Unicode byte/character boundaries and selection replacement.

### composer-09: Context is mostly a shelf or serialized trailer rather than inline records

P2 | partial-capability | source-proven

Users cannot place an image, quote, review note or browser annotation at a precise point in instructions, move it as one inline object, or copy the object with its context to another draft.

Trigger: Compose before/after instructions for two images and a quote, then try to reorder or copy their references between drafts.

Expected: Keep typed context records with stable identities and optional inline positions, supporting preview, movement, deletion and clipboard round trips.

Actual: Files, replies, review comments and browser annotations occupy separate vectors/stores. Replies precede the line; review text follows it; files become trailing @paths. The editor token kinds are File, Command and Skill, rather than the full context record set.

Cause: The persisted and transmitted form is flattened prompt text. Shelf order and serialization order substitute for explicit inline context identity.

Ferrite evidence:

- `crates/ferrite-pane/src/composer.rs:139-166,536-554,1142-1155`.
- `crates/ferrite-core/src/prompt_files.rs:15-24`.
- `crates/ferrite-reviewr/src/ferrite.rs:538-547`.
- `crates/ferrite-pane/src/attachments.rs:89-127,149-189`.

T3 evidence at the pinned commit:

- [apps/web/src/composerDraftStore.ts:2600-2621](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2600).
- [packages/contracts/src/composerContext.ts:1-180](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/composerContext.ts#L1).
- [docs/user/composer.md:323-379](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L323).

Recommendation: Introduce structured context records while keeping a plain-text fallback for providers. Preserve positions and clipboard metadata without requiring every attachment to become a new visual component.

Observable acceptance check: Write instructions around two image chips and a quote; cut/paste them into a second draft and restart. Each preview, source and relative position survives, and the outgoing prompt identifies the intended object correctly.

### composer-10: The mention menu has no PR, thread or model-reference search

P2 | missing-capability | source-proven

Users must manually find and describe another thread, PR or model. Structured references that an agent can inspect on demand are missing from the composer.

Trigger: Type #123, @part-of-thread-title or @opus in a bound composer, or drag a sidebar thread onto it.

Expected: Offer distinct PR, thread and model-reference results with previews and stable source identities, without changing the active model when mentioning it.

Actual: derive_menu recognizes @files, $skills and /commands. The @ branch resolves a workspace root and performs file search. No PR trigger, thread/model search branch or thread drop target appears in the composer path.

Cause: The menu model and prompt context codec were built around file and skill names. Thread identifiers, repository PR identities and model references have no composer item type.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:6288-6307,6492-6595`.
- `crates/ferrite-pane/src/composer.rs:1142-1155`.
- `crates/ferrite-pane/src/prompt_drop.rs:11-45`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/useComposerMenuItems.ts:60-79,120-180,323-348](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L60).
- [apps/web/src/components/chat/composerModelMentions.ts:1-43](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/composerModelMentions.ts#L1).
- [packages/client-runtime/src/composerThreadItems.ts:1-115](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/client-runtime/src/composerThreadItems.ts#L1).
- [docs/user/composer.md:332-360](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L332).

Recommendation: Add reference-specific item types and payloads. Reuse PR/thread identity from their owning modules rather than scraping displayed text. Keep linked-machine transport separate from local thread references.

Observable acceptance check: Pick a PR by number and text, a thread by title, and a model by alias. Each chip previews the right source; model mention leaves active model unchanged; the agent receives a resolvable reference. A sidebar thread drop has the same result as a menu pick.

### composer-11: Review comment drafts lose their structured controls after reopen

P2 | defect | source-proven

Review comments survive as prose but no longer have their original review attachment controls after the pane is recreated. This is loss of editability and source structure, not loss of the comment words.

Trigger: Add a review comment to a composer, close its pane or restart, then return before sending.

Expected: Restore the review comment as the same pending review object, with source anchor and removal/edit controls.

Actual: keep_draft stores composer.prompt, which exports comments into ordinary text. Composer.set parses browser annotations, file trailers and quotes, but never reconstructs CommentStore. On a fresh Composer its review store is empty and exported review prose becomes editable message text.

Cause: The draft format uses the outbound prompt serialization, and the comment export has no matching structured restore step at this seam.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/drafts.rs:28-56`.
- `crates/ferrite-pane/src/composer.rs:494-513,536-549`.
- `crates/ferrite-reviewr/src/ferrite.rs:538-547`.
- `crates/ferrite-pane/src/composer.rs:354-369,427-432`.

T3 evidence at the pinned commit:

- [apps/web/src/composerDraftStore.ts:2600-2621](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2600).

Recommendation: Persist comments separately from outbound prompt text and hydrate them with the other draft context records.

Observable acceptance check: Add two anchored review comments, restart, and remove just one through its original control. The remaining comment retains its file/line anchor and sends once; neither appears as duplicated raw export text.

### composer-12: There is no prompt stash independent of the active draft

P3 | missing-capability | source-proven

Users cannot temporarily set aside several prompts and their context while composing something else. Per-thread autosave and sent-prompt history do not provide this workflow.

Trigger: Compose an unsent task with attachments, save it aside, send a different task, then restore the first.

Expected: A stash preserves unsent text/context independently, lists entries and restores them without unexpectedly sending.

Actual: Ferrite has one stored string per ThreadId and history recall. No composer stash model, action or menu was found. T3 exposes a persistent stash workflow with readiness checks and restoration.

Cause: Draft persistence represents the current draft only, with no saved-prompt collection.

Ferrite evidence:

- `crates/ferrite-core/src/drafts.rs:12-17,44-58`.
- `crates/ferrite-cockpit/src/cockpit.rs:6617-6650`.
- `crates/ferrite-pane/src/composer.rs:41-80`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/useComposerStash.ts:461-522,549-596,698-720](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerStash.ts#L461).
- [apps/web/src/components/chat/ComposerStashMenu.tsx:1-150](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerStashMenu.tsx#L1).

Recommendation: Treat stash as an optional product addition after draft durability. Reuse structured draft snapshots and explicit deletion/retention rules.

Observable acceptance check: Stash two prompts including images and quotes, restart, compose a third, then restore either stash. No item sends implicitly and attachment expiry/failure is visible.

### composer-13: The native composer lacks rich Markdown and code-block editing

P3 | intentional-difference | source-proven

Formatting is plain source text with token highlighting. Users do not get T3 list continuation, task nesting, code indentation, syntax highlighting or language selection while composing.

Trigger: Type a Markdown list, quote and fenced code block, then use Enter, Shift+Enter and Tab inside them.

Expected: If adopting T3 parity, provide optional rich Markdown editing while keeping the exact outbound text stable and a plain-text mode.

Actual: Ferrite uses Line content and token/selection/IME styling. Newline inserts a hard break with no Markdown structure. T3 conditionally installs Markdown and code-block extensions and handles indentation/list continuation.

Cause: Ferrite deliberately uses a plain native editor. This is a valid simpler product model unless rich composition is a requirement.

Ferrite evidence:

- `crates/ferrite-pane/src/line.rs:1-32`.
- `crates/ferrite-pane/src/composer.rs:787-790,1273-1310`.

T3 evidence at the pinned commit:

- [apps/web/src/components/ComposerPromptEditorTiptap.tsx:810-870,1271-1318](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L810).
- [docs/user/composer.md:15-36](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L15).

Recommendation: Keep plain text as a supported choice. If richer editing is desired, first define exact Markdown round-trip and shortcut behavior, especially Enter inside code.

Observable acceptance check: Lists nest/continue predictably, code blocks indent and choose language, and toggling rich/plain preserves the exact submitted Markdown. Test IME, Unicode, multiline selection and undo across formatting boundaries.

### composer-14: There is no integrated voice-input workflow

P3 | missing-capability | source-proven

T3 users can dictate into a draft with recording, transcription and cancellation feedback. Ferrite has no corresponding composer control or state. OS-level dictation may still work and was not tested.

Trigger: Try to start microphone input from the composer or its shortcuts.

Expected: An optional voice flow records only with user action, inserts a reviewable transcript at the saved selection and supports cancel/error recovery.

Actual: No microphone, dictation or transcription path was found in the native composer/control sources. T3 has explicit idle, preparing, recording, transcribing, post-processing and error presentations.

Cause: The input model currently accepts keyboard/IME, clipboard and file intake only.

Ferrite evidence:

- `crates/ferrite-pane/src/composer.rs:41-80,594-619`.
- `crates/ferrite-cockpit/src/cockpit.rs:8291-8301,8525-8541`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/ComposerSpeechButton.tsx:16-39,91-119,122-145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerSpeechButton.tsx#L16).
- [docs/user/composer.md:199-203](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L199).

Recommendation: Treat this as a product capability choice. If added, reuse explicit draft ownership and cancellation rules; disclose the transcription destination.

Observable acceptance check: Start voice input, dictate, finish, and verify text lands at the saved selection without sending. Cancel and microphone failure preserve the original draft. Changing threads does not insert text into the wrong draft.

### composer-15: A first prompt cannot fan out to multiple chosen models

P3 | missing-capability | source-proven

A user comparing models must manually create and submit several threads. T3 has a single draft action that launches selected models in isolated worktrees.

Trigger: On a new Git-backed draft, select two models and send one prompt to both.

Expected: If parity is desired, create separately identified threads/worktrees with the same prompt/context and independently recover partial failures.

Actual: DraftBinding contains one ProviderChoice and effort; bootstrap resolves and creates one thread. T3 validates a multi-model selection and routes it to sendToMultipleModels.

Cause: Ferrite treats a draft as the future of exactly one thread. Groups and workflow automation are separate mechanisms and do not provide this composer action.

Ferrite evidence:

- `crates/ferrite-core/src/draft.rs:27-35`.
- `crates/ferrite-cockpit/src/cockpit.rs:7854-7877,7895-7904`.

T3 evidence at the pinned commit:

- [apps/web/src/components/chat/composerSend.logic.ts:39-59](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/composerSend.logic.ts#L39).
- [apps/web/src/components/chat/useComposerSend.ts:1110-1130](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1110).

Recommendation: Keep this optional and separate from ordinary send. If adopted, model a fan-out submission explicitly instead of overloading one thread binding.

Observable acceptance check: Send one draft to two models. Two isolated, clearly labeled threads receive identical prompt/context. Failure of one leaves its submission recoverable without resending the successful one.

## Supported T3 capability inventory

These are implementation/documentation capabilities at the pinned commit, not promises that every deployed client or provider supports every option.

| Capability | Comparison result |
| --- | --- |
| Existing-thread and pre-thread drafts | Ferrite persists existing-thread text/context serialization; new-pane durability is missing. T3 persists both draft content and draft-session bindings. [apps/web/src/composerDraftStore.ts:2263-2371](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2263). |
| Send/queue/steer | Both support active-turn follow-ups. T3 exposes user timing intent; Ferrite uses provider-native behavior. [apps/web/src/components/chat/ComposerPrimaryActions.tsx:250-265](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPrimaryActions.tsx#L250). |
| Stop/held queue/Resume | Ferrite intentionally continues surviving queue work. T3 exposes held queue resumption. [apps/web/src/components/chat/useComposerResume.ts:52-55,88-166](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerResume.ts#L52). |
| Queue edit/remove/send now | Both have visible row actions. T3 edit is transactional; Ferrite takes back and merges. [apps/web/src/components/chat/useQueuedRunEdit.ts:162-219,322-380](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useQueuedRunEdit.ts#L162). |
| File, image, screenshot, paste and drag intake | Both support local intake. T3 also prepares images, supports HEIC/HEIF conversion, validates limits and uploads files to the execution environment. [apps/web/src/components/chat/useComposerAttachmentIntake.ts:247-284](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L247); [docs/user/composer.md:38-56](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L38). |
| Attachment previews | Ferrite shelf items preview images/files. T3 additionally describes rendered/raw text views, audio/media playback and ZIP browsing. These preview details are owned by the panel-tools slice. [docs/user/composer.md:381-407](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L381). |
| Large-paste file conversion | T3 has automatic text attachments and a keep-inline override; Ferrite inserts the text. [apps/web/src/components/chat/useComposerAttachmentIntake.ts:371-445](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L371). |
| Rich Markdown and code composition | T3 optional; Ferrite plain editor is a valid deliberate difference. [apps/web/src/components/ComposerPromptEditorTiptap.tsx:810-870,1271-1318](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L810). |
| `/` commands and `$` skills | Both present provider commands/skills. T3 can hide skills from slash menus and separates provider commands that must lead a prompt. Ferrite also guards local actions from consuming inline prose. [apps/web/src/components/chat/useComposerMenuItems.ts:271-321](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L271); `crates/ferrite-cockpit/src/cockpit.rs:6309-6311`. |
| `/compact`, `/clear`, restart session and goals | T3 documents all, with provider restrictions. Ferrite has provider-native command/control paths, so these must be assessed by provider rather than counted absent from a missing literal UI string. Goal/session lifecycle coverage belongs to providers-settings. [docs/user/composer.md:273-321](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L273). |
| `@` workspace files | Both support search. Ferrite has provider search plus local fallback with a 2,000-file cap; remote drafts without a provider search do not use the local walk. See `crates/ferrite-cockpit/src/cockpit.rs:6492-6595,9357-9359`. |
| `#` PRs and `@` threads/models/machines | T3 supports typed entities, direct PR number lookup, linked threads and model reference hints. Ferrite composer search is files/skills/commands. [apps/web/src/components/chat/useComposerMenuItems.ts:120-180,323-348](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerMenuItems.ts#L120); [docs/user/composer.md:332-360](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L332). |
| Inline context and clipboard round trips | T3 context chips carry their records through copy/paste/stash. Ferrite has shelf context and plain-text serialization. [apps/web/src/composerDraftStore.ts:2600-2621](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2600); [docs/user/composer.md:323-379](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L323). |
| Cite response and annotate quote | Ferrite can render preloaded reply comments, but Cite creates an empty comment and exposes no comment editor. This is incomplete authoring, documented in [quote-attachments-001](quote-attachments.md). T3 retains a source-linked citation and provides a comment editor. [docs/user/composer.md:129-146](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L129); `crates/ferrite-pane/src/composer.rs:645-654`; `crates/ferrite-pane/src/attachments.rs:551-618`. |
| Prompt history and suggestions | Both support them. Ferrite protects Tab acceptance from implicitly sending. T3 suggestions are opt-in/provider-version dependent. [docs/user/composer.md:148-169](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L148); `crates/ferrite-cockpit/src/cockpit.rs:5732-5777`. |
| Edit earlier prompt and retry | Ferrite supports capability-gated rewind/edit/fork and retry. Its Codex adapter cannot restore files; this is a declared provider limitation, not missing Edit. T3 has worktree checkpoint restoration with shared-directory protection. [docs/user/composer.md:171-186](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L171); `crates/ferrite-core/src/providers/codex.rs:673-678`. |
| Prompt stash | T3 supports several saved prompts and context with upload/expiry handling. Ferrite has no independent stash. [apps/web/src/components/chat/useComposerStash.ts:461-596](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerStash.ts#L461). |
| Voice input | T3 web/desktop has explicit recording/transcription states. Ferrite has no integrated flow. Native OS dictation is unverified. [apps/web/src/components/chat/ComposerSpeechButton.tsx:16-39,91-145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerSpeechButton.tsx#L16). |
| Model, effort, speed and permissions | Both expose model/effort/permission controls. T3 consumes descriptors such as service tier and provider-specific thinking modes. Ferrite model/effort picker is substantial; finer provider-option parity belongs to providers-settings. [apps/web/src/components/chat/ComposerModelEffort.tsx:39-46,99-131](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerModelEffort.tsx#L39); [apps/web/src/components/chat/ComposerFooterControls.tsx:24-45,105-130](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerFooterControls.tsx#L24). |
| Multiple-model first prompt | T3 routes one draft to several isolated model runs. Ferrite one draft creates one thread; groups/workflows remain separate. [apps/web/src/components/chat/useComposerSend.ts:1115-1130](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1115). |
| Mobile offline and system-share input | T3 mobile saves local attachment copies, queues offline, handles share sheets and uses native dictation. This is platform breadth, not a defect in Ferrite desktop. [docs/user/composer.md:93-98,205-244](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/docs/user/composer.md#L93). |

## Positive Ferrite behavior

- Existing-thread drafts use atomic file replacement and clear on send. This protects ordinary draft recovery when the admission path succeeds. See `crates/ferrite-core/src/drafts.rs:34-58` and `crates/ferrite-cockpit/src/cockpit/drafts.rs:28-64`.
- First-send bootstrap preserves a failed draft and does not erase edits made while startup runs. See `crates/ferrite-cockpit/src/cockpit.rs:7854-7932`.
- File/image paste prefers external paths over duplicate path text. Screenshots without files become attachments. CRLF and CR normalization preserves multiline pasted text. See `crates/ferrite-pane/src/composer.rs:587-619`.
- Drop hit testing is pane-scoped and explicitly addresses native drag keyboard-modality rejection. See `crates/ferrite-pane/src/prompt_drop.rs:17-45`.
- Menu parsing uses the caret position, distinguishes slash-containing paths from slash commands, displays command discovery state and avoids local action consumption inside prose. See `crates/ferrite-cockpit/src/cockpit.rs:6288-6311,6373-6393`.
- Atomic tokens, native text selection, IME marked ranges, word motions and undo/redo are implemented. This is a real native editor, not a decorative input. See `crates/ferrite-pane/src/line.rs:13-42` and `crates/ferrite-pane/src/composer.rs:41-80,1273-1310`.
- Native queue correlation avoids automatic resends after uncertain partial pipe writes. See `crates/ferrite-core/src/engine.rs:3752-3791`.
- Rewind cannot race a running turn, queued input, pending request or model change. Failed-turn retry includes attachments. See `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:25-46,169-190`.
- Suggestions require Tab to place text and a separate send. See `crates/ferrite-cockpit/src/cockpit.rs:5732-5777`.
- Useful composer motion exists: height changes retarget from their current position and honor reduced motion; recently added file chips grow in; send glyph changes between send and steer. See `crates/ferrite-pane/src/composer.rs:694-707`, `crates/ferrite-pane/src/attachments.rs:139-146` and `crates/ferrite-pane/src/pane/composer.rs:250-284`. Queue insertion/removal intentionally snaps according to `crates/ferrite-pane/src/pane/composer.rs:781-790`. The motion slice should judge whether this meets the user's request, rather than treating a historical reference's lack of motion as decisive.

## Verification gaps and proposed runtime order

1. Reproduce the two conditional prompt-loss paths first in disposable threads. Verify a graceful restart, not only a forced crash, for a new draft. Exercise early root/start refusal separately from normal provider write failure, which has a recovery path.
2. Attach a uniquely named client-only file to a remote machine with no shared filesystem. Test picker, screenshot paste and native drop separately. Keep browser capture out of this test because it has a different transfer path.
3. Confirm active-turn queue/steer, Edit, Send now and Stop on both installed provider versions. Source support does not prove the CLI advertises the required native queue lifecycle or that a row receives its click.
4. Round-trip file, quote, review and browser-annotation drafts through pane close and app restart. Check source identity, individual removal and duplicate text, not only text equality.
5. Exercise command and mention menus with mouse and keyboard on a new draft and a live thread, including loading/error, remote search and catalogs that change while open.
6. Test model/effort changes during idle and busy periods, permission controls, first-send branch/worktree options and failure recovery. Coordinate with providers-settings and projects-navigation to avoid destructive checkout tests.
7. Validate native drag/drop, clipboard formats, IME, undo, long multiline input and focus boundaries with input-focus. No claim in this report establishes those live interactions passed.
8. Inspect the optional T3-only workflows with real provider capability gates before adopting them. Rich formatting, voice and multi-model send require product decisions and are lower priority than preserving current work.

Existing relevant tests include `crates/ferrite-cockpit/src/cockpit/tests/drafts.rs`, `composer_controls.rs`, `composer_chips.rs`, `composer_menus.rs`, `queued_prompts.rs`, and provider wire tests. The drafts test module opens with existing-thread coverage and constructs `Drafts` by `ThreadId`; it does not establish pre-thread durability. No tests were run for this report, and no passing test is represented as a UI reproduction.

## Recommended sequence

First make draft ownership and send admission reliable. Then fix remote attachment transfer and expose attachment readiness. Decide queue/steer and Stop semantics before expanding queue UI; transactional Edit depends on that decision. Preserve structured context across persistence and clipboard boundaries before adding entity mentions or stash. Treat rich Markdown, voice and multi-model submission as optional follow-on product work, with the current plain native workflow retained where appropriate.
