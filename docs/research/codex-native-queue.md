# Codex native prompt queue

Verified 2026-09-06 against installed `codex-cli 0.153.4`. Research only; no UI
or production provider changes. Branch `investigate/native-provider-queues`.

## Decision

Use the app-server queue as the authority. Ferrite should submit through
`thread/queue/add`, observe/reconcile the server queue, and invoke its cancellation
and edit methods. It must not drain a local FIFO with `turn/start` after each
completion. The existing provider input conversion remains the correct seam.

### Amendment 2026-09-10: steer while a turn runs

`thread/queue/add` only dispatches when the thread is idle, so a prompt sent
mid-turn waited for the whole turn. The Codex CLI's own Enter during a turn is
`turn/steer` (`codex-rs/tui/src/chatwidget/input_flow.rs` at rust-v0.153.4):
the server folds the input into the running turn at its next tool boundary.
Ferrite now steers whenever it knows the active turn id, and falls back to
`thread/queue/add` when the steer is refused for "no active turn" or an
expected-turn mismatch — the queue then starts it as soon as the thread idles.

`turn/steer {threadId, expectedTurnId, clientUserMessageId, input}` answers
`{turnId}`; consumption is observed as a `userMessage` item carrying `clientId`,
the same correlation the queue path uses. A steer has no server handle and
cannot be deleted; Ferrite mirrors it under the id `steer:<clientUserMessageId>`.

On interrupt the server discards un-consumed steers (the TUI's
`input_restore.rs` says so and restores them locally). Ferrite instead does
what Claude Code's Escape does: when the interrupted turn ends, everything still
mirrored is resent as one `turn/start`, oldest first. Native queue entries in
that pile are deleted first so the server's queue cannot run them again.

## Evidence

- Installed `codex app-server generate-json-schema --experimental --out <scratch>`
  includes the six queue requests and `thread/queue/changed`.
- Installed `codex queue --help` exposes `--thread` and `--message`.
- Reproducible [probe](../../scripts/probe-native-codex-queue.py) drives the **real
  installed app-server**, with an isolated `CODEX_HOME`, temporary cwd, read-only
  sandbox, and a localhost mock Responses endpoint. It uses no credentials, real
  model requests, or project tools. The mock only provides final assistant text;
  it does not implement queue behavior. [Observed results](native-queues/codex-0.153.4.json).
- Version-pinned [protocol](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/app-server-protocol/src/protocol/common.rs),
  [queue service](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/ext/queue/src/service.rs),
  [request processor](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/app-server/src/request_processors/thread_queue_processor.rs),
  [upstream integration tests](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/app-server/tests/suite/v2/thread_queue.rs).
- The [CLI queue command](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/session_queue_commands.rs)
  calls `ThreadQueueAdd`. Interactive TUI input also has local staging state;
  that is not a reason to reimplement the provider's available queue service.

Run from this worktree:

```sh
python3 scripts/probe-native-codex-queue.py > /tmp/codex-queue-result.json
python3 scripts/probe-native-codex-queue.py --crash > /tmp/codex-queue-crash.json
```

## Verified contract

| Operation | Installed behavior |
|---|---|
| Capability | Requires `initialize.capabilities.experimentalApi: true`. Without it, queue/list returns -32600 naming that capability. |
| Enqueue while busy | `thread/queue/add {threadId,input,clientUserMessageId}` returns `queuedSubmission {id,input,clientUserMessageId}`. All three prompts appeared in server order. |
| Start while busy | `thread/queue/start` returns -32600; existing queue remains. |
| Edit | `thread/queue/update {threadId,queuedSubmissionId,input}` retains queue item ID. |
| Delete | `thread/queue/delete {threadId,queuedSubmissionId}` returns `deleted:true`; repeat returns `deleted:false`. |
| Reorder | Requires every remaining item ID exactly once; a partial list was rejected. Valid reorder was reflected by queue/list. |
| Attachment | `localImage` is snapshotted into an `image` data URL at enqueue. Deleting the original file after acknowledgment does not remove its queued content. Missing file returns an error before admission. |
| Retry identity | Two adds with the same `clientUserMessageId` created **two different queue IDs**. This is correlation, not an idempotency key. |
| Interrupt | Kept the two queued prompts. |
| Process restart | queue/list, before thread/resume, returned both original IDs and edited input in the same order. |
| Resume after interrupt | Both remained pending during the two-second observation window. Explicit queue/start succeeded. Do not assume thread/resume always unpauses interrupted work. |
| Native dispatch | After that one queue/start, **two turns completed** in server queue order, with no client turn/start or second queue/start. Queue/list became empty. |
| Correlation | Completed userMessage items carried `clientId` equal to submitted `clientUserMessageId`. |
| Notifications | `thread/queue/changed` arrived for queue mutations and consumption. Its payload names the Thread, not a full queue snapshot. |

The probe waits for the real first `turn/started` before exercising busy controls.
A successful `turn/start` response precedes that event; racing interrupt into
that interval can return "no active turn to interrupt". Client readiness must
follow provider events, not merely the request's synchronous return.

A second [SIGKILL probe](native-queues/codex-crash-0.153.4.json) recovered both
pending queue items via queue/list in a new process, but immediate thread/resume
returned `already has an active writer`. Do not equate durable queue recovery
with immediate execution availability. The writer recovery delay was not measured;
retain the native queue and surface/retry the native resume refusal instead of
starting a duplicate conversation.

## Lifecycle and failure implications

The pinned service's `enqueue` stores before waking a loaded Thread. Its lifecycle
extension dispatches on idle, except interrupted idle. It also observes queue
changes/resumed Threads. `start` uses `start_turn_if_idle` and removes the queue
entry **when the turn starts**, not when the model completes. A later model
failure therefore does not mean the item is still queued. These are source-backed
semantics; the probe exercised successful model completion and interruption,
not every possible model-error branch.

The server snapshots local image/audio inputs, validates input size, and owns
queue capacity and storage. Other file references retain their normal input-item
semantics; an arbitrary mentioned file is not promised an immutable snapshot.

Pagination is explicit (`cursor`, `limit`, `nextCursor`). A queue-change event
requires a fresh list, including all pages. Serialise refreshes/coalesce changes
so an older list response cannot overwrite a newer view.

On disconnect after an uncertain add, **do not blindly retry**: list persisted
items and reconcile by client ID first. If an item has already started, consult
provider userMessage/history correlation before deciding it was never accepted.
On an uncertain delete, refresh before restoring text to the Composer. False
means the item may already have started; it must not be restored as unsent.

## Ferrite integration, keeping UI intact

1. Enable experimental API in the existing Codex handshake; detect queue support
   through a read-only queue/list request after Thread creation/resume. The current
   Ferrite minimum 0.149.1 is not proven to support this; do not infer support from
   the broad 0.x version window or silently fall back to a local scheduler.
2. Add asynchronous queue operations to the existing provider Session interface.
   Reuse `wire::input_items` for text, skills, mentions and attachments. Do not
   construct a second provider transport or SDK instance.
3. Mirror acknowledged server items for the current queued row. Keep client and
   server queue IDs distinct. Route its existing unqueue action through native
   delete, waiting for success before changing the Composer.
4. Fold queue changes and actual prompt-start observations into typed SessionEvents.
   Queue admission is not a sent transcript prompt. Deduplicate history and local
   transcript insertion using provider correlation, including after reconnect.
5. Remove Cockpit's turn-end queue drain. The provider chooses when work starts.
   Preserve existing ordinary first-send/context/handover preparation exactly once;
   queued wire text may differ from the operator-facing raw text.
6. Account for parked/revived Sessions: provider pending work is durable and may
   resume execution when loaded. Do not describe it as discarded at park. Clear
   the local view only as a view, not as cancellation of native pending work.

No claim of exactly-once delivery across arbitrary crashes. Duplicate-client-ID
behavior makes reconciliation necessary; a queue mirror is justified, a second
scheduler is not.
