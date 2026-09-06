# Native provider queues: backend investigation

2026-09-06. Branch `investigate/native-provider-queues`, worktree
`.worktrees/native-provider-queues`, based on `main`. **Existing UI unchanged.**
No production code changed; the earlier Ferrite-owned FIFO branch is not the
recommended implementation.

## Finding

Both installed providers supply a native queue and targeted cancellation. Ferrite
can retain its current Composer, queued row, and keyboard controls while replacing
local scheduling with provider operations and an acknowledged display mirror.
It does not need another SDK dependency or a second provider process.

| | Codex 0.153.4 | Claude Code 2.1.263 |
|---|---|---|
| Add pending input | `thread/queue/add` | Existing stream-json user frame, with `uuid` and `isAsync:true` |
| Authority | Persisted server queue; `thread/queue/list` | `command_lifecycle` per submitted UUID |
| Remove pending input | `thread/queue/delete` | `cancel_async_message` control request |
| Edit | Native update retains position/ID; current take-back UI can delete then edit | Cancel pending UUID, then return its text to current Composer |
| Already started | Delete may return false | Cancel returns `cancelled:false` |
| Scheduling | Provider automatically starts queued turns when eligible | Provider chooses consumption boundaries and can batch multiple prompts in one turn |
| Restart | Pending items survived graceful stop and SIGKILL. Immediate resume after SIGKILL hit an active-writer refusal in the probe. | Acknowledged pending input was lost in the tested SIGKILL/resume case. |
| Discovery | Experimental handshake plus read-only queue/list capability probe | `msg_lifecycle_v1` and related init capability tokens; installed protocol verified |

Detailed claims, source citations and observed outputs:
[Codex](codex-native-queue.md), [Claude](claude-native-queue.md).
The reproducible probes exercise installed CLIs, not Ferrite's fake Sessions.
Codex uses a localhost fake model; Claude uses tools-disabled, temporary-workspace
Haiku text tasks. Committed outputs omit account/config metadata.

## Backend change to make next

1. Extend the existing provider Session abstraction with enqueue/cancel operations
   and typed queue acknowledgments/lifecycle updates. Keep provider wire handling
   inside its current module and reuse attachment/input conversion.
2. Submit busy prompts immediately to the native queue. Maintain a **display
   mirror**, keyed by native IDs, rather than a local dispatch queue. Temporary
   unsent input before Session startup is a pending client operation, not an
   acknowledged provider queue item.
3. Let the provider report consumption. Remove Cockpit's queue release on
   `TurnEnded`. In particular, never assume one Claude result consumes one prompt.
4. Keep the existing empty-Composer Enter/Backspace interactions. Their backend
   must wait for successful native cancellation before retrieving/removing text.
   If cancellation loses a race with start, refresh the existing view and do not
   create a duplicate draft or resend.
5. Record transcript/history at actual prompt consumption, separately from queue
   admission. Correlate provider events and suppress duplicates. Preserve the
   existing split between displayed operator text and hidden first-send/handover
   context. A local metadata journal is acceptable for correlation; it must not
   become a second execution queue.
6. Reconcile Codex's native snapshot on reconnect. For Claude, do not imply
   durability or silently retry uncertain submissions. Use the existing Notice
   surface for lost/uncertain pending work and unsupported capabilities.
7. Keep provider-native differences: batching, interruption and persistence are
   not identical. Do not implement a common FIFO scheduler to erase them.

## Checks required while implementing

- Replay the committed native lifecycle fixtures through each real adapter, then
  through Cockpit; prove that a turn end never sends a queued prompt itself.
- Cover failed admission, pending cancellation, lost cancel races, stale replies
  after Session replacement, and exact-once transcript/history insertion.
- Codex: all-page list reconciliation; queue-change races; duplicate client ID
  submissions are not idempotent; interrupted queue restart; active-writer refusal.
- Claude: multiple UUIDs consumed by one result; queue lifecycle arrives outside
  ordinary transcript events; cancellation preserves the active turn and siblings.
- Reuse current UI tests for queue/unqueue. No visual redesign or new queue widget
  is needed for this backend work.

## Remaining limits

This investigation establishes the viable native seams and the important behavior
differences. It does not establish the earliest supporting CLI release, every
possible provider failure event, or exactly-once crash recovery. Current Ferrite
version floors are older than the versions probed; capability handling is required.
The precise Codex active-writer recovery delay after SIGKILL remains unmeasured.
These are implementation/test boundaries, not reasons to keep the local FIFO.
