# Claude native prompt queue

Investigated 2026-09-06, on branch `investigate/native-provider-queues`. Backend research;
no changes to the existing Composer UI.

Run the sanitized, tools-disabled live harness with
`python3 scripts/probe-native-claude-queue.py --mode cancel` (also `batch`,
`interrupt`, `persistence`, `image-cancel`). It uses authenticated model calls deliberately.
Checked-in captures under `docs/research/fixtures/` omit account and environment
metadata; they preserve only command IDs, lifecycle, result correlation and
control receipts.

## Conclusion

**Use Claude's queue, including its cancellation and lifecycle protocol.**
Ferrite already holds open the same stream-json transport the Agent SDK uses.
It needs to stamp submitted messages with UUIDs and `isAsync: true`, consume
`command_lifecycle` events, and send `cancel_async_message` control requests.
It does not need a separate SDK process, SDK dependency, or Ferrite turn scheduler.

This conclusion is backed by successful live probes of the installed **Claude
Code 2.1.263**, not merely the terminal UI documentation. The official published
**Agent SDK 0.3.263** package contains the cancellation wire type and its runtime
implementation. The Python SDK's public client does not expose that convenience
method; that does **not** mean the CLI protocol lacks it.

## Sources and reproducibility

- Local `claude --version`: `2.1.263 (Claude Code)`; executable resolves to
  `~/.local/share/claude/versions/2.1.263`.
- Official [SDK package 0.3.263](https://registry.npmjs.org/@anthropic-ai/claude-agent-sdk/0.3.263),
  [published tarball](https://registry.npmjs.org/@anthropic-ai/claude-agent-sdk/-/claude-agent-sdk-0.3.263.tgz).
  Read `package/sdk.d.ts`: `SDKControlCancelAsyncMessageRequest`,
  `SDKControlInterruptRequest`, `SDKControlInterruptResponse`, `SDKUserMessage`,
  `SDKResultSuccess`, `SDKResultError`. `package/sdk.mjs` implements
  `cancelAsyncMessage(uuid)` by sending the control request below and returning
  `response.cancelled`. Some runtime methods/fields are omitted from the public
  TypeScript declarations; treat this as a capability-checked wire integration.
- Official [Python client](https://github.com/anthropics/claude-agent-sdk-python/blob/efd4d865ef1795daffee3cd24cce45307aed8a51/src/claude_agent_sdk/client.py)
  `query()` writes each user message directly to the CLI transport.
  [Internal query](https://github.com/anthropics/claude-agent-sdk-python/blob/efd4d865ef1795daffee3cd24cce45307aed8a51/src/claude_agent_sdk/_internal/query.py)
  `stream_input()` likewise writes each yielded message. Its shutdown comment
  explicitly distinguishes messages queued CLI-side from result boundaries.
- [Streaming input docs](https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode)
  document queued input and images. This broad description does not promise one
  result for every submitted prompt.
- [Terminal queue behavior](https://code.claude.com/docs/en/interactive-mode#queue-messages-while-claude-works)
  describes same-turn delivery after tools, and terminal take-back. Terminal
  keybindings are not the headless API contract.

Local probe scripts/captures were written only under
`/tmp/ferrite-claude-queue-probe/`: `probe.py`, `cancel.py`, `batch.py`,
`interrupt.py` and their `*-capture.jsonl` outputs. Captures include ordinary
CLI initialization metadata and are intentionally not copied into the repo.
Each process used an isolated temporary working directory, no tools, no session
persistence, and a tiny Haiku text task:

```sh
claude -p --input-format stream-json --output-format stream-json \
  --verbose --include-partial-messages --replay-user-messages \
  --safe-mode --strict-mcp-config --tools '' --model haiku \
  --no-session-persistence --system-prompt 'Reply exactly as requested. No tools.'
```

Initialize once; keep stdin open. Send the first request asking for numbers
1–100. On its first `stream_event`, send SECOND and THIRD before any result.
No model could modify the project: tools were disabled and cwd was temporary.

## Verified protocol

The installed CLI's `system/init` advertises:

```json
["interrupt_receipt_v1", "interrupt_cancel_queued_v1", "msg_lifecycle_v1"]
```

These arrive on `system/init`, not the initialize control response. Ferrite's
existing provider comment already identifies that distinction.

Submit immediately, including when Main is running:

```json
{"type":"user","uuid":"<client UUID>","isAsync":true,"message":{"role":"user","content":"Reply exactly SECOND."}}
```

Observed events have their own envelope UUID, plus a separate command UUID:

```json
{"type":"command_lifecycle","command_uuid":"<client UUID>","state":"queued","uuid":"<event UUID>","session_id":"<session UUID>"}
{"type":"command_lifecycle","command_uuid":"<client UUID>","state":"started","uuid":"<event UUID>","session_id":"<session UUID>"}
{"type":"command_lifecycle","command_uuid":"<client UUID>","state":"completed","uuid":"<event UUID>","session_id":"<session UUID>"}
```

Plain UUID-stamped messages without `isAsync` were processed but did **not**
emit these lifecycle events in the first probe. `--replay-user-messages` echoed
them only when consumed; a replay is not an immediate queue acknowledgment.

Cancel one pending prompt, preserving the running turn and other pending input:

```json
{"type":"control_request","request_id":"cancel","request":{"subtype":"cancel_async_message","message_uuid":"<client UUID>"}}
```

Pending cancellation produced a `command_lifecycle` with `state:"cancelled"`,
then:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"cancel","response":{"cancelled":true}}}
```

Cancelling an already-started command returned `cancelled:false`. Treat that as
lost cancellation race, not success. Do not restore the prompt into Composer
unless cancellation is confirmed. Read timeout or broken pipe is also not proof
of cancellation.

The stronger interrupt operation was separately verified:

```json
{"type":"control_request","request_id":"interrupt","request":{"subtype":"interrupt","cancel_queued":true}}
```

Both pending UUIDs received `cancelled` lifecycle events, then the receipt listed
`{"still_queued":[],"cancelled":["<second UUID>","<third UUID>"]}`. This is a
stop-everything operation, not the implementation of individual take-back.
The package contract says ordinary interrupt preserves queued messages and
reports survivors in `still_queued`; that variant was not repeated live here.

## Scheduling is provider-owned

Two back-to-back pending messages were **combined into one next turn**, both
with and without `isAsync:true`. The async probe emitted `started` for both
UUIDs. Its result contained:

```json
{"user_message_uuid":"<third UUID>","user_message_uuids":["<second UUID>","<third UUID>"],"result":"SECOND.\nTHIRD."}
```

Thus three submitted prompts produced two results. The SDK type comments
explicitly describe this batching and additional same-turn messages folded in
between tool rounds. Such messages can grow the result's UUID list beyond the
first reply's list. Same-turn tool folding was not exercised with a live tool
in this tools-disabled probe.

`queued_turn_count` was **0 on the first result even though submitted follow-ups
subsequently ran**. Do not derive Ferrite's mirror by popping one prompt per
result or clearing it on this count. Use per-command lifecycle. The package
says the count concerns pending sends at result construction; the observed
ordering is another reason not to substitute it for command lifecycle.

## Backend integration, existing UI retained

1. Extend the existing Claude provider send path with client UUID and
   `isAsync:true`; retain `wire::input_content()` for attachments.
2. Maintain only display/acknowledgment bookkeeping in Ferrite. The CLI owns
   scheduling; never resend a held prompt on `TurnEnded`.
3. Add provider events for queued/started/terminal command lifecycle. Remove a
   pending row on that UUID's `started` event, including multiple started
   events for a batch. Do not confuse Main's queue with subagent activity.
4. Existing retrieve/remove gestures call `cancel_async_message` on the newest
   pending UUID. Return the text only after `cancelled:true` or an authoritative
   matching cancelled event. Native cancellation is also sufficient for edit
   by take-back and resubmit; no native in-place edit/reorder operation was
   identified or verified.
5. Keep pending-submit metadata until acknowledgment so a failed write does not
   silently discard the user's prompt. A successful pipe write is not provider
   acceptance. Do not auto-retry uncertain delivery, which could duplicate work.
6. Feature-check `msg_lifecycle_v1` after the first `system/init`. Do not claim
   all supported 2.x CLIs support this seam solely because 2.1.263 does.
   Existing 2.1.243 fixture `claude-hello-2.1.243.jsonl` already announces the
   three tokens, but announcement alone is not a live compatibility test for
   that version. The current provider floor 2.1.224 does not guarantee them.
   Unsupported providers should report unsupported queuing rather than quietly
   install a local scheduler.

## Remaining limits

- **Persistence/reconnect:** a separate probe enabled persistence, received a
  queued acknowledgment for SECOND while FIRST streamed, then killed only its
  own CLI. Resuming the same native session produced no automatic work during
  eight seconds after initialize. A fresh question about the pending message
  answered NO. Inspection of that probe's own transcript found no pending UUID
  or pending prompt: only FIRST, a synthetic continuation and the new question.
  Thus pending input did not survive this 2.1.263 crash/resume case. No headless
  queue-list/snapshot API was found in the SDK surfaces inspected. Do not
  replay uncertain entries automatically: at a different crash instant they
  could already have been consumed. Restore uncertain text as a draft only
  through an explicit recovery decision; do not claim a durable Claude queue.
- **Attachments:** a text + valid 1×1 base64 PNG user message received a native
  queued acknowledgment, then targeted cancellation returned true and emitted
  cancelled before image consumption. The running text prompt completed alone.
  See `claude-native-queue-image-cancel-2.1.263.jsonl`; image bytes are omitted.
  This verifies native admission/cancellation of the existing content shape,
  not model vision or eventual attachment consumption. Reuse Ferrite's existing
  adapter (ADR-0004), rather than adding another production encoding path.
- **Errors:** cancellation success/failure was observed; model errors, hook
  rejection, permission denial and lifecycle error-state spelling were not
  induced. Unknown lifecycle values must not be silently called completed.
- **Lifecycle after result:** probes terminated once their target result or
  control response was read. Do not infer absent subsequent terminal events
  from these truncated captures. Earlier command completion was observed.
- **SDK API stability:** cancellation exists in shipped runtime and wire types,
  while `isAsync`/lifecycle are not fully described in the public declarations.
  Pin this behavior with recorded protocol tests and an opt-in live test, and
  detect announced capability tokens rather than treating terminal docs as a
  version guarantee.
