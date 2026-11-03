# Edit prompt and rewind

Research for two features. **Edit**: change a sent prompt and re-run from there, as Codex does. **Rewind**: go back to a prompt, drop everything after it, and optionally restore files, as Claude Desktop does. Checked 2026-11-03 against Claude Code **2.1.317** and codex-cli **0.166.0**. The sources are binary strings, the generated app-server schema, the public docs, and local session files. Ferrite has no rewind, fork or truncate today. The "transcript fork action" does not exist yet: the only `head_is_fork` in the tree is the PR-forge field.

## Mechanisms

### Claude Code

| Mechanism | Status | What it does |
|---|---|---|
| `control_request {subtype:"rewind_conversation"}` | Undocumented, in the stdin dispatcher | Cuts the live conversation in place, in the same `else if` chain as `interrupt`, `set_model`, `rewind_files` and `cancel_async_message`, which Ferrite already uses. |
| `control_request {subtype:"rewind_files", user_message_id, dry_run?}` | Public (SDK `rewindFiles`) | Restores the files to their state at that prompt. Returns `{canRewind, error?, filesChanged?, insertions?, deletions?, skippedLinks?}`. |
| `--resume <sid> --resume-session-at <uuid> [--resume-drops-turn <prompt uuid>]` | Hidden flag; public as SDK `resumeSessionAt`/`resumeDropsTurn` | At spawn, keeps "only messages up to and including the chain entry with <message.id>". Print mode only. |
| `--fork-session` (with `--resume`) | Public | Writes to a new session id. A fork does not carry file checkpoints ("you can't rewind it to a checkpoint captured before the fork", SDK `forkSession` docs). |
| `--rewind-files <user-message-id>` (with `--resume`) | Public, hidden from help | A one-shot file restore that exits. |
| `control_request fork_conversation` | Remote-control only | Answers `unsupported / requires_remote_control_server` unless `--sdk-url` is set. Not usable. |

**`rewind_conversation`** has the following request and response:

- **Request fields** (read from the handler): `target_message_uuid` (required), `last_seen_user_message_uuid`, `interrupt_if_running`, `allow_summarized_target`, `allow_assistant_target`, `refuse_if_usage_limited`, `expires_at`.
- **Success:** `{rewound:true, targetMessageUuid, prefillText, precedingAssistantUuid, resolution?}`. `prefillText` is the target prompt's text, for the composer.
- **Refusal:** `{rewound:false, prefillText:null, error, reason}`, where `reason` is one of:
  - `commands_queued`
  - `prompt_pending`
  - `turn_running`
  - `target_not_found`
  - `stale_target`
  - `unseen_later_turn`
  - `target_splits_tool_call`
  - `persist_failed`
  - `state_changed`
  - `usage_limited`
  - `expired`

The handler persists the cut (`persistLocal`, and `persist_failed` is one of the refusals). The schema text for the refusal event names "the rewind target and composer prefill for edit-and-retry", which is exactly the Edit flow. Of the two rewind requests, only `rewind_files` has a published Zod schema. `rewind_conversation` is internal.

**File checkpointing in `-p`** is off unless an environment variable is set. The binary's own gate:

```
f0(){ … if(ge()) return hHo(); return fileCheckpointingEnabled && !DISABLE }
hHo(){ return CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING && !CLAUDE_CODE_DISABLE_FILE_CHECKPOINTING }
```

Without the variable, `rewind_files` answers "File rewinding is not enabled." The docs (agent-sdk/file-checkpointing, /checkpointing) list what a restore covers:

- It restores only what the main thread changed with Write, Edit or NotebookEdit. A file that only Bash touched is not restored, and neither are subagent edits (except a foreground forked skill).
- A message sent mid-turn has no checkpoint of its own.
- Only the last 100 checkpoints are kept, for about 30 days.
- Symlinks and hard links are skipped (`skippedLinks`).

**Prompt identity.** The id that both rewinds take is the user entry's `uuid`, and the CLI keeps a uuid the host supplies:

- Ferrite's `enqueue` sends `"uuid": client_id`, and that same uuid becomes the entry's id. The evidence is `~/.claude/projects/-Users-josephwylie-Desktop-Projects-ferrite/6bb4432c-….jsonl`, whose user entry `000005ed-0020-4000-8023-26217bf2b7f0` matches the format of `engine.rs::client_id`.
- Ferrite's `send` path (`claude.rs::send`) sends no uuid, so the CLI generates one that Ferrite never sees.

### Codex app-server (0.166.0)

`codex app-server generate-json-schema [--experimental]` lists `thread/fork` and `thread/revert`. `thread/rollback` is gone: there is no such method string in the binary, only the `ThreadRollbackFailed` error kind. There is no undo: `codex features list` shows `undo  removed`.

- **`thread/revert {threadId, beforeTurnId}`** (stable). The schema describes it as: "Replace a paginated thread's durable history with the prefix before one turn. This only changes persisted conversation history. **It does not revert local file changes.**"
  - It returns `{thread (turns empty), turnsBackwardsCursor?, itemsBackwardsCursor?}` and emits `thread/reverted {threadId}`.
  - It works in place on a loaded thread.
  - It errors with "thread … does not use paginated history" on legacy threads.
  - On this machine, `~/.codex/state_5.sqlite` (read-only query) has 452 legacy threads, all at 0.153.4 or older, and 1863 paginated threads (the newest are 0.166.0). Old imported or revived Codex Threads will refuse it.
- **`thread/fork {threadId, lastTurnId? (stable) | beforeTurnId? (experimental), excludeTurns?, model?, cwd?, …}`**. It returns a new thread.
  - `beforeTurnId` excludes that turn and every later one. `lastTurnId` keeps turns through the one named, which cannot be in progress.
  - Ferrite already initializes with `experimentalApi:true` (`codex.rs:449`), so `beforeTurnId` is available.
- **Turn ids.** The `turn/start` response and `turn/started` carry the turn id. Ferrite sees them (`codex.rs` around line 1250) but does not record which prompt started which turn.
- **Files.** Codex never restores files. Ferrite has the material to do it: the store persists every `turn/diff/updated` as `Record::TurnDiff {turn_id, diff}` (`store.rs:389`). The live Transcript keeps only the latest diff (`transcript.rs:707`). Reverse-applying the dropped turns' diffs, newest first, after a `git apply -R --check`, would restore what Codex's own turn diff tracked.

### What the handoff doc says

`docs/research/claude-desktop-transcript-handoff.md` touches this in three places:

- Line 74: "File rewind | Checkpointing/rewind support | Selection UI and coordination with conversation history". The CLI does the restore; the host does the selection and the coordination.
- Line 171: Desktop runs with `enableFileCheckpointing: true`.
- Lines 103 and 268: "A conversation fork does not fork the filesystem."

## Recommended design

The same verbs work for both providers:

- **Rewind(prompt, restore_files)**: cut the conversation in place, before that prompt.
- **Edit(prompt)**: Rewind, then put the prompt's text back in the Composer. Ferrite already has a path for handing a prompt back: `ThreadState::recovered` and `Op::TakeRecovered`, which the CancelQueued `edit` uses. The operator edits and sends.
- **Fork(prompt)**: a new Ferrite Thread whose history is the prefix and whose native conversation is a provider fork.

Every verb is refused while a turn runs, unless the operator chooses to interrupt first.

### Claude

1. **Give every prompt a Ferrite uuid.** Make `send` carry `"uuid"` the way `enqueue` does, then record it (see the store section).
2. **Spawn with `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=1`.** Without it, checkpoints never exist.
3. **Rewind:**
   - If restoring files, first send `rewind_files {user_message_id, dry_run:true}` to show "N files, +a −d" for confirmation, then send it again for real.
   - Then send `rewind_conversation {target_message_uuid, last_seen_user_message_uuid: <latest prompt uuid>}`.
   - On `rewound:true`, cut the Transcript and log. Treat `prefillText` as the Composer text for Edit.
   - On a refusal, surface `reason`.
4. **Fallback if `rewind_conversation` is missing or refused as unknown:**
   - Respawn with `--resume <sid> --resume-session-at <uuid of the entry before the target>`.
   - Add `--resume-drops-turn <target>` when exactly one turn is dropped.
   - The kept entry is the target's `parentUuid` in the CLI's session JSONL, or `precedingAssistantUuid`.
5. **Fork:** spawn the new Thread's Session with `--resume <sid> --fork-session --resume-session-at <parent of target>`. Its file checkpoints start empty, so a fork offers no file restore for earlier prompts.

### Codex

1. **Record each prompt's turn id**, taken from the `turn/start` response.
2. **Rewind:** send `thread/revert {threadId, beforeTurnId}`, then cut the Transcript and log. If the thread is legacy (the "does not use paginated history" error), fall back to the fork below and rebind the Thread to the new id.
3. **Fork:** send `thread/fork {threadId, beforeTurnId, excludeTurns:true}`, which gives a new Ferrite Thread bound to the returned id.
4. **Files:** ship v1 as conversation-only and say so in the confirmation. Later, restore from Ferrite's own record: reverse-apply the persisted `TurnDiff`s of the dropped turns, refusing on any conflict.

### Store and transcript

- **Rewinding keeps the log append-only.** It appends a `Record::Rewind {prompt}` (schema 15), the same way `Handover` changes how replay reads earlier lines.
- **Prompt identity in the log.** Name the prompt by its ordinal among `Prompt` records, so the cut stays meaningful in old logs. Store the native id next to it by extending the `PromptObservation` input with `native_id` (the Claude uuid or the Codex turn id).
- **Replay drops the cut prompt and everything after it.** It calls a new `Transcript::truncate_from(prompt block)`. The cut prompt can sit in an evicted range, so the cut must work from the log, not only from the live window.
- **Recall** (Up/Down through sent prompts) and the queue must drop the rewound prompts too.
- **A fork is a new Thread.** The store copies the source log through the record before the cut prompt, using the existing `checkpoint`/`LogAt` offsets. The header's `resume` is the forked native id.

### Protocol, engine, UI

- **Protocol:** add `Op::Rewind {thread, prompt, files: bool, edit: bool}` and `Op::Fork {thread, prompt}` to `protocol.rs`, with replies carrying the provider's refusal `reason`. The server/window wire needs the matching version bump.
- **Engine:** add `Session::rewind(target, files) -> Result<RewindOutcome>` and `Session::fork_args(target)` in `providers/mod.rs`, plus a `ControlKind::Rewind` capability, so a provider or version without support disables the action. The engine then appends the record and cuts the transcript. For Edit it sets `recovered`, and for Fork it provisions the new Thread.
- **UI:**
  - The prompt bubble's `message_actions()` in `ferrite-pane/src/transcript.rs` (around line 947, today the sent time plus Copy) gains **Edit**, **Rewind** and **Fork from here**.
  - **Rewind** opens a small ChoiceMenu: *Conversation*, or *Conversation and code* when the dry run reports changes.
  - Matching palette commands go next to "Copy prompt" and "Resend prompt" in `ferrite-cockpit/src/cockpit/palette.rs`, acting on `reading_prompt`.

## Files to touch

- `crates/ferrite-core/src/providers/claude.rs`: the env variable at spawn, a uuid on `send`, the `rewind_files` and `rewind_conversation` requests and their replies, and the fallback and fork spawn args.
- `crates/ferrite-core/src/providers/claude/wire.rs`: the reply types.
- `crates/ferrite-core/src/providers/codex.rs`: the prompt-to-turn-id map, `thread/revert`, `thread/fork`, and handling for `thread/reverted`.
- `crates/ferrite-core/src/providers/codex/wire.rs`: the same reply types for Codex.
- `crates/ferrite-core/src/providers/mod.rs`, `session_event.rs`: the `Session` trait methods and `ControlKind`.
- `crates/ferrite-core/src/protocol.rs` (+ `protocol/tests.rs`): the two ops and their replies.
- `crates/ferrite-core/src/engine.rs`, `engine/commands.rs`, `engine/queue.rs`: the rewind, edit and fork flows, and clearing the queue and recall.
- `crates/ferrite-core/src/store.rs`: the `Rewind` record, the prompt `native_id`, schema 15, the prefix copy for a fork, and a compat test.
- `crates/ferrite-core/src/transcript.rs`: `truncate_from`, replay of the cut, and the prompt identity on the Prompt block.
- `crates/ferrite-pane/src/transcript.rs`: the bubble actions.
- `crates/ferrite-cockpit/src/cockpit/palette.rs`, `transcript_glue.rs`: the commands, the menu, and the Composer prefill.
- New capture fixtures under `crates/ferrite-core/tests/fixtures/` for each provider's rewind and fork.

## Risks

- **`rewind_conversation` is internal and undocumented.** Before building on it, pin it with a captured fixture, as was done for `--permission-prompt-tool stdio`. The probe costs no model call: resume a `--fork-session` copy and send only control requests. Keep the respawn fallback, which is public through the SDK.
- **Prompts Ferrite cannot name.** Prompts sent before the uuid change, and prompts from logs written before then, have no native id, so rewind must be disabled on them.
  - For Claude, a recovery is possible: match the text against the CLI's session JSONL.
  - A queued prompt that was absorbed mid-turn has no file checkpoint.
- **Partial file restore.** Claude restores only main-thread edits made with Write, Edit or NotebookEdit. Codex restores nothing. The confirmation must say what will not come back, and point to git.
- **Legacy Codex threads** (0.153 and older) cannot `thread/revert`. The fork fallback changes the native thread id under the Thread, so the binding and resume metadata have to follow. `beforeTurnId` on fork is experimental, while `lastTurnId` is the stable alternative.
- **Running turns, queues, Decisions and subagents.** Claude refuses with `turn_running`, `commands_queued` or `prompt_pending`. Ferrite should interrupt or ask first, never race. Subagents that are still running when the cut happens are orphaned.
- **Ordering.** A file restore followed by a refused conversation rewind leaves the code and the history out of step.
  - The design restores the files before rewinding the conversation, because it is not known whether `rewind_files` still finds the target's checkpoint once `rewind_conversation` has removed that message. The capture probe must settle this.
  - Until then, check the refusals Ferrite can see before restoring any files: a running turn, queued commands, a pending Decision.
- **Machines.** The ops cross the server/window wire, and a rewind from an older window must be refused cleanly.
