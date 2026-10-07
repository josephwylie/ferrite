---
status: accepted
date: 2026-10-07
---

# Store IO off the UI thread

The [`Store`](../../crates/ferrite-core/src/store.rs) is a synchronous module
driven from the pump, so every append, flush, sync, header rewrite, load and
peek runs on the thread that handles keystrokes. Three roots make that
expensive, and this ADR addresses each of them:

1. All file IO runs on the UI thread.
2. Mutable header facts live in line 1 of an append-only log. Renaming a
   Thread, moving its worktree or changing its provider therefore rewrites
   the whole log.
3. Reviving a Thread replays the whole log, and parses it three times to do
   so, even though Activity keeps only the last 4 MB.

## Context

The figures below come from a read-only copy of a real 127 MB Thread log
(189,510 records, schema 11), release build on macOS 27. The page cache is
warm unless a row says otherwise.

| Operation | Today |
|---|---|
| `fs::read` of the log | 9 ms (55 ms cold) |
| `Store::load` (one full parse) | 132–156 ms |
| `Store::writer` (load + `has_torn_tail` reparse) | 260–274 ms |
| Revive's store reads (`load`, then `writer`: three full parses) | ≈400 ms |
| `set_title` (load, reserialize, F_FULLFSYNC, rename) | 264–278 ms |
| Revive replay: `activity_inputs` + `Activity::apply` × 189,522 | **32–42 s** |
| `peek` (header line) | 0.04 ms |
| Append one 300 B line: write / fsync / F_FULLFSYNC | 26–41 µs / 25–96 µs / 3.0–4.2 ms |

Under write load, F_FULLFSYNC (Rust's `sync_data` on macOS) has a p95 of
24–61 ms and a maximum of 2.6 s. Plain `fsync` has a p50 of 0.03 ms and a
maximum of 0.5 s.

The replay row covers more than revive. `LogReader::subagent_count` runs the
same replay for every parked Thread whenever the parked set changes (see
`Facts::parked_changed` → `ParkedLookups::run`). With the operator's 42
Threads (420 MB) this keeps a core busy long after launch, at about 1 GB RSS.
Track C is fixing the rebuild storm itself. The store must make parked facts
cost O(header) regardless.

A throwaway lab test (not committed) cut the log at the first Main prompt
within a 16 MB window, with the base at 115.6 MB:

- **Tail alone:** reading and parsing 11.8 MB took 12–17 ms and replay took
  17–20 ms. The 55 retained Main blocks were identical to a full replay, but
  `context_details` differed, because its newest record sits 12.7 MB from
  EOF.
- **Tail plus carried records:** adding the newest prefix record of each
  latest-wins kind (7 records, parsed in 0.02 ms) made Main blocks, model,
  session id, usage, usage details and context details all identical. Replay
  took 26 ms.

This log's full replay has no children, so it does not exercise the child
carry (class (c) in section 3). The property test must.

A reverse scan to the newest record of a kind costs whatever distance it has
to cover: 0.07 MB for `init`, 12.7 MB / 17 ms for `context_details`, and
24.8 MB / 34 ms for `conversation_reset`.

## Decision

The log stays the single durable truth about a Thread, and every read and
write of it moves behind one store worker. The log grows two new record
kinds:

- **`facts` amendments.** Header facts are appended, never rewritten.
- **`mark` records.** A mark is a derived bookmark. It restates the facts,
  carries the parked summary, and names a replay base with its carry index.

The schema becomes **13**. Callers keep the `Store` and `ThreadWriter` types.
The methods that rewrote the log are deleted, and the seven separate reads
that revive makes are deepened into one call.

### 1. One store worker behind the Store interface

**One writing process per store.** Two Ferrites can share a store (a dev
build beside the installed app; both default to `~/.ferrite/threads`). If
both write, one can truncate the other's committed records, strand its
appends behind a rename, or interleave marks. To prevent that:

- `Store::open` takes an exclusive, non-blocking lock on `<store>/.lock`
  through `File::try_lock`, which is `flock` on Unix and `LockFileEx` on
  Windows.
- A process that cannot get the lock opens the store **read-only**. It
  lists and peeks Threads but refuses create, revive, amendments, repair
  and upgrade with `StoreError::ReadOnly`, and the window says so.
- Stores are keyed by the directory's device and inode, so every handle in
  one process shares one lock, one meta cache and, later, one worker.
- Temporary files carry the process id and a counter, never a shared name.

Each store directory gets one worker per process. `Store::open` hands out
clones of a shared inner, so `Groups`, the history loader and `LogReader`
reach the same worker. The worker owns every log handle and runs commands in
FIFO order. Each log's effects therefore land in the order its callers issued
them, exactly as the synchronous calls do today. Three threads share the
work:

- **The worker** writes and coalesces.
- **A sync thread** runs `fsync` and F_FULLFSYNC on cloned handles, so no
  command ever waits on a drive cache.
- **The reader** handles long reads: revive tails, hand-over carry and child
  history. It is the existing ADR-0002 loader, generalized.

What callers get:

| Caller act | Contract after this ADR |
|---|---|
| `record_event`, `record_prompt`, `record_prompt_observation`, `record_completion` | Returns at once. The record is converted to the store's schema on the caller and queued. Coalescing, boundary flushes and the 5 s interval move into the worker unchanged. `Err` reports the writer's current failure. The record is still accepted, as today. |
| `set_title`, `set_workspace`, `set_provider` | Returns at once. Enqueues only the **change** (a delta). The worker applies it to its own fold of the Thread's facts and appends the full result as a `facts` record. A caller's stale view therefore cannot undo an earlier change, such as a provider switch committed just before a rename. No `writer` parameter is needed, and it is removed. The meta cache that `peek` reads is updated before returning. |
| `peek` | Served from the in-process meta cache. The worker warms it for every Thread at open. A miss reads the header plus the tail back to the newest facts-bearing record, which section 3 bounds. A miss never waits on the worker: an amended Thread is always cached. |
| `ThreadWriter::health()` (new) | Answers `Ok`, `Backlogged` or `Failed(message)` without blocking. |
| child-history checkpoint | Asynchronous. A `Checkpoint` command writes the Thread's buffered records, then forwards the ADR-0002 request to the reader with `through` = committed length. The cockpit inserts its `Pending` buffer when it enqueues. Everything accepted after the barrier lands past `through` and replays once from the buffer, the same contract as today. If the writer is `Failed`, the checkpoint answers `Err` for that serial and the cockpit drops its `Pending`. |
| `park` | Hands the writer to the worker, which writes, appends a mark, F_FULLFSYNCs and removes `.open`. Park no longer waits. A failure is retained and reported (below). `mark_open` and `mark_parked` travel through the same FIFO with a per-Thread generation, so a park's late marker removal can never delete the marker of a revive that came after it. |
| `hand_over` | Asynchronous commit (section 2). |
| `revive` | One call, `Store::revive(id) -> Revival` (section 3). It waits only for a write barrier and a bounded read. |
| `delete`, `flush` (import, tests), quit | Blocking barriers through the FIFO. `delete` closes the handles before `remove_dir_all`, which Windows needs. `flush` returns when its records are durable. Quit is bounded (E1, below). |

**Ordering.** There is one FIFO per store. An amendment, a handover or a mark
lands between the same two records it would land between today.

**Checkpoint barrier.** A barrier makes earlier writes readable, because the
page cache is coherent after `write(2)`. It does not wait for syncs.

**Store-error backpressure.** `flush_records`, `PendingFlush` and the
`DurableWrite` seam move into the worker verbatim. A failed append keeps its
encoded bytes and the offset it reached. The worker retries with backoff.
Nothing is dropped and nothing is re-encoded.

**How the pump learns of failures.** The pump reads `writer.health()` every
frame where it calls `writer.flush()` today. While a writer is `Failed`, the
pump reports the error once (`report_store_error`) and stops draining that
Session. The bounded provider channel then absorbs events, as it does today.
`Backlogged` holds draining in the same way: it fires when more than 16 MiB
of accepted-but-unwritten records are queued, so a slow disk cannot grow
memory. Failures on parked Threads have no owner to report to. They arrive
through `Store::take_failures()`, which the pump also polls. The meta cache
entry is dropped so `peek` re-reads the truth.

**Park and revive after a failure.** A revive waits until the Thread's
retained records are written. It refuses with the store error while they
cannot be written. Today it is the park that refuses in that case.

**Quit and E1.** Today `halt_sessions` never flushes writers, so ⌘Q loses up
to 5 s of records per Thread. Under this ADR, `halt_sessions` calls
`Store::close_all(QUIT_DEADLINE)`:

- It always waits for every accepted record to be **written**. A record
  that passed a boundary must survive a Ferrite crash, so writes are never
  abandoned.
- It writes a mark and requests F_FULLFSYNC. The deadline (proposed 500 ms)
  bounds only that **sync**.
- A writer that is `Failed` at quit saves its pending encoded bytes to
  `log.pending-<pid>-<n>.jsonl` beside the log. The next launch reports
  that file in a Notice and never merges it automatically.
- A panic hook asks the worker to drain before the process unwinds.

Data that has passed `write(2)` survives process exit. Only power loss
inside the deadline can cost the unsynced tail. If the disk stalls and the
writes cannot finish, the quit waits. A force quit is the operator's
choice, never Ferrite's.

### 2. Header facts as appended amendments

Line 1 is written once, by `create` or by the one-time upgrade below. After
that, the facts change by appending a record:

```json
{"type":"facts","provider":"claude","workspace":{"kind":"main","checkout":"/r"},
 "session_project_root":null,"model":null,"project_id":14,"title":"Fix nav","effort":"high"}
```

A `facts` record restates every fact, not a delta, so the newest one alone is
enough.

**Fold rule.** Readers start from the header, then apply each record in
order. A record that carries facts replaces them all:

- `facts` carries them.
- A schema-13 `handover` carries them in a `facts` field, beside `from`, `to`
  and `model`.
- `mark` restates the fold up to itself; a test pins that equality.

A pre-13 `handover` carries no facts and changes nothing, because its effect
is already in the header that was rewritten with it. Folding it again would
clobber later changes, such as a model picked after the switch.

`load` and `peek` share one implementation of the rule.

**`peek`.** On a cache hit, peek is a memory read. On a miss it reads line 1
for the schema. Below 13, line 1 is the answer. Otherwise it scans back from
EOF for the newest `facts`, `mark` or `handover` record. A handover keeps the
scan going until the next facts-bearing record, then applies on top of it.
The mark policy (section 3) bounds this scan to about 1 MiB plus one flush
batch, which is never the 12–25 MB measured above. `peek_first_prompt` is
unchanged.

**Commit path.** Amendments and handovers are never retried from a buffer:
a caller told that a change failed must not see it land later. They take a
commit path instead:

1. Flush the buffer.
2. Write the line and sync it.
3. On any failure, truncate back to the committed length, through a
   separate `write(true)` handle, because an append-only handle cannot
   truncate on Windows.

If that truncation also fails, the writer becomes `Failed` and refuses
appends until the log is reopened and repaired.

**`hand_over`.** The new provider and its Handover must commit together. One
line does that: the schema-13 `handover` record carries the new facts, so no
second record is needed. The worker runs the commit in three steps:

1. Flush, then compute the exchanges from the last `conversation_reset` up to
   EOF with today's `AnswerText` logic. This is a full read, never a bounded
   one, so the exchanges are complete. If it fails, nothing has changed.
2. Append the `handover` line and fsync it. If that fails, truncate back to
   the committed length, so the old log and Session stay authoritative.
3. Answer with the `Handover`.

The cockpit gets that answer at commit time in `advance_startups`, where
replacements already settle asynchronously. Until the answer arrives it stops
draining the old Session. Otherwise a late old-provider `Init` could land
after the `handover` line and become the resume target.

**Schema and old logs.** Schema 13 adds `facts` and `mark`. Logs at schemas
1–12 load unchanged, because their header is their only facts record.

**Upgrade.** The first append to a pre-13 log upgrades it once. This happens
on the worker, at revive or at the first amendment. The upgrade:

1. Verifies the log with one full parse.
2. Writes `log.jsonl.tmp` containing a schema-13 header, then every record's
   **original bytes verbatim** up to the last readable line, then a mark.
3. F_FULLFSYNCs it and renames it over the log.

Unlike today's upgrade, nothing is reserialized, so the copy is lossless by
construction. It costs one O(log) pass per old log, once. Compaction is
neither needed nor proposed: amendments are about 300 B, and the newest one
always wins.

**Why not a sidecar.** A `meta.json` beside the log is O(1) too, and an older
Ferrite would simply ignore it. It is rejected for four reasons:

- It creates two sources of truth that a crash can split.
- It breaks the atomic Handover-with-provider commit.
- It leaves the log no longer self-describing.
- CONTEXT.md names the header as the durable truth.

Two more alternatives were rejected:

- **An in-place header slot** (a padded line 1 rewritten with `pwrite`, or
  patching `"12"`→`"13"` in place). A torn sector write would destroy the one
  line that has nothing before it to recover to.
- **Encoding facts as extra fields on an old record kind**, so that old
  readers ignore them. No old kind is inert on replay.

### 3. Marks: O(header) parked facts and O(retained) revive

```json
{"type":"mark","facts":{…},"summary":{"turns":11,"subagents":3,"prompted":true},
 "cost":91234567,"prev":126800211,"base":{"carry":[102648572,114702941,…]}}
```

**When the worker writes a mark.** At the first flush boundary after
`max(1 MiB, 16 × last mark size)` bytes since the previous mark, and always
at `create`, park, quit and upgrade. That keeps the overhead below 6.25% and
bounds the tail that `peek` and the summary must scan. Every mark follows a
barrier sync. A mark therefore never reaches the disk ahead of the records
it vouches for.

**Replay base.** A mark's `base` names a replay base **at any offset**:
`base: {at, carry}`. `at` is a turn start, meaning a Main `prompt` written
while Main was idle. Claude's queued prompts land mid-turn and do not
count. `carry` lists the offsets of the earlier records a replay from `at`
needs.

- The live writer keeps the carry state for recent turn starts. Each mark
  names the newest turn start with at least 2 × the Main content budget of
  Main cost after it.
- The one-time upgrade computes the same thing from its full parse. A
  Thread's first revive after the upgrade is therefore already bounded,
  which is what the 42 existing logs need.

**Field contents.**

- `cost` is the cumulative Main retained cost. It is computed with Activity's
  own `input_bytes`, exposed `pub(crate)` so there is one function and no
  drift.
- `prev` links each mark to the one before it.
- `summary`:
  - `turns` counts prompts since the last reset, the same count
    `LogReader::turn_count` gives today.
  - `prompted` is the `history_locks` fact.
  - `subagents` comes from the live Activity through
    `ThreadWriter::note_summary`. That keeps Activity the only authority on
    child identity. The count is absent until one has been noted, for
    example in the mark that an upgrade writes.

**Parked facts become O(header).** `ThreadMeta` gains `summary`. `peek` adds
prompts and resets from the short suffix after the newest mark.
`LogReader::subagent_count` and `turn_count` read the summary, and
`ParkedLookups` never replays a schema-13 log. For pre-13 parked logs the
choice is an open question (see the end). The recommendation is a *derived*
summary cache file in the store directory, computed once per (Thread, length,
mtime) in the background and freely deletable, like `rate-limits/`.

**Bounded revive.** `Store::revive(id)` works in four steps:

1. Behind a write barrier, repair the tail and upgrade the log if it is old.
2. Find the newest mark by reverse scan and take its base.
3. Read the records named in the base's `carry` by offset, then read
   `[at, EOF)` sequentially.
4. Open the append handle.

The returned `Revival` replaces the separate `load`, `writer`,
`resume_target`, `last_handover`, `prompt_texts`, `inputs` and
`activity_inputs` calls that revive makes today. It contains:

- the facts;
- the resume target;
- prompt recall;
- `first_prompt_sent`;
- the owed Handover, if one is undelivered;
- the replay inputs;
- the writer.

An undelivered Handover still computes its exchanges with a full read, so
invariant 3 holds.

**Carry classes.** The store owns this classification of its own record
kinds:

| Class | Records carried |
|---|---|
| (a) latest-of-kind | The newest `init`/`conversation_reset`/`handover` (resume), `model_changed`, `token_usage`, `context_usage`, `usage_details`, `context_details`, `run_state`, `turn_ended`, `turn_diff` |
| (b) fold families | The newest plan, tasks snapshot and background snapshot, plus the deltas after each |
| (c) child identity | Per canonical child: its first introduction, every alias, its newest status, coverage and outcome, and any detached record |
| (d) recall | Every `prompt` since the last reset |

**Why the replay is equivalent.** The Main window is a suffix, and trimming
only pops from the front. So any replay that contains every record from a
turn start at or before the window start ends with the same retained Main
records. `rebuild` keeps the runtime that the carried records set.

Children introduced before the base are restored with identity, status and
outcome intact. They are evicted only **after** the whole replay, so content
in the suffix cannot reattach to an already evicted child. Selecting one
reloads its content through the ADR-0002 loader, which reads from disk off
the paint path. The property tests cover:

- a small `max_children`;
- aliases that cross the base;
- whichever trimming Activity uses. Track C is moving it to incremental
  eviction, so this ADR keeps its `activity.rs` changes to a minimum. With no usable
mark (a crash before the first mark, or a carry offset that fails to parse
as its declared kind), revive falls back to a full replay. Correctness never
depends on the accelerator.

Persisting Activity's derived runtime in the mark was rejected. It would
make internal state, which changes freely, part of the schema. Carry offsets
name records the log already holds.

**Repair without a full parse.** After a crash, only the bytes after the
newest intact mark are in doubt. Barrier syncs order every write, so a
power cut can leave a hole only after the last barrier, never before a mark
that reached the disk. Repair runs in four steps:

1. Parse every line from the newest mark that parses to EOF.
2. If all of them parse, no repair is needed. A final record missing only
   its newline gets the newline appended.
3. Otherwise, copy every byte from the first unreadable line onward to
   `log.damaged-<offset>.jsonl` and sync the copy. If the copy fails,
   repair aborts and **truncates nothing**.
4. Truncate to the end of the last good line, then sync.

Line length never counts as damage. A tool-output line can exceed any
window.

A mark certifies that every line before it parsed when it was written. That
is what lets the bounded reader keep `load`'s "the first unreadable line is
the end" meaning without reading the prefix. A pre-13 log has no marks, so
its first repair or upgrade parses it whole, once.

**ADR-0002 caches.** Byte offsets no longer move after an amendment, so the
`history.clear()` invalidations in `apply_move`, `rename_thread`, `set_model`
and `set_effort` become unnecessary. They stay in place until the tests prove
that is safe.

### 4. Sync policy and durability contract

| Moment | Sync |
|---|---|
| Boundaries: turn end, `closed`, `handover`, child terminal facts, amendments, the 5 s interval, and before every mark | Barrier: `fcntl(F_BARRIERFSYNC)` on macOS, `fdatasync` elsewhere on Unix |
| Park, quit, upgrade, `create` | F_FULLFSYNC |
| Any log written since its last full sync | F_FULLFSYNC every 30 s |

Plain `fsync` is not used at boundaries on macOS. It does not order writes,
so after a power cut a later block could persist without an earlier one,
leaving a hole that a last-line check would miss. A barrier costs about
what `fsync` does.

All syncs run on the sync thread. On Windows every level is
`FlushFileBuffers`, which does order writes.

The durability contract this implies:

- A record past a boundary survives a Ferrite crash or a kernel panic.
- After power loss, everything up to the last full sync survives, and
  nothing after it can come back out of order. At worst, a sudden power cut
  loses up to 30 s of records.
- The log stays consistent in every case, because repair handles the cut.

`queue.json` keeps its synchronous save, with its refusal contract. It uses
a barrier before its rename. An unreadable `queue.json` now produces a
Notice instead of silently starting empty.

### 5. Migration and rollback

**Unchanged behaviour.**

- Pre-13 logs load unchanged and are never written until a revive or
  amendment upgrades them.
- An upgraded log keeps every record's bytes.
- Torn-tail repair removes only bytes that never formed a committed record.

**Interior damage.** A full parse can find an unreadable line that is not
the last one. Today's `writer()` silently discards everything after such a
line. Instead, the bytes to be cut are copied to `log.damaged-<offset>.jsonl`
and synced first, so no byte is lost. A hard link would not work, because
truncation changes the inode it shares. If the copy fails (NTFS has no
clone, so this is a full copy), the repair aborts. The Thread then stays
readable but is not reopened for writing.

**Windows.**

- Truncation uses a separate `write(true)` handle.
- `delete` waits until the worker, the sync thread and the reader have all
  dropped their handles.
- Rename-over uses `MoveFileExW(MOVEFILE_REPLACE_EXISTING |
  MOVEFILE_WRITE_THROUGH)` instead of a directory fsync. A directory fsync
  exists only on Unix.

**Downgrade.** An older Ferrite refuses a schema-13 log whole
(`FutureSchema`). Its `writer`, `set_*` and `load` all read the schema
first, so nothing gets rewritten. However, an older `Groups::load` peeks
every member, so one upgraded Thread inside a Group stops an older Ferrite
from opening the store at all. Every past schema bump had the same effect.
Logs that have not been upgraded stay readable by the older version.

### 6. Agent-context invariants

1. **Every logged event is still logged, in order.** The same
   `Record::from_event` and coalescing run on the same call order through one
   FIFO. Failures retain records and hold draining. Quit now flushes (E1).
   New kinds are additive.
2. **Resume ids persist exactly as today.** `init`, `conversation_reset` and
   `handover` are unchanged and always carried. `resume_target` keeps its
   rule. A handover commit holds the old Session's events until the
   `handover` line lands.
3. **`hand_over` receives the complete exchange history.** Exchanges are
   always computed by a full read from the last reset. The bounded window is
   never used for them.
4. **No existing log is rewritten lossily.** Amendments and marks only
   append. The setter rewrite path is deleted. Upgrades copy bytes verbatim.
   Repair truncates only an uncommitted tail. Damaged originals are kept.

### 7. Tests and commit order

**Guards that stay green.** These existing tests keep their behaviour:

- **Flush policy:** `no_durable_write_ever_occurs_per_delta`,
  `a_long_turn_flushes_on_the_interval_not_per_delta`.
- **Torn tail:** `a_crash_torn_tail_never_loses_the_thread`,
  `appending_after_a_crash_never_hides_the_new_turn`,
  `streaming_child_reader_recovers_torn_tail_and_refuses_future_header`.
- **Old schemas:** `a_log_written_at_schema_v1…v7_still_loads_after_the_bump`,
  `appending_to_a_v1_thread_upgrades_its_log_first`,
  `schema_eight_upgrade_retains_main_history_before_new_attributed_facts`.
- **Downgrade fence:** `a_log_from_ferrites_future_is_refused_not_half_read`,
  `peek_refuses_a_future_schema_like_load_does`.
- **Invariant 3:** `a_handover_is_replayed_and_shadows_the_old_providers_init`,
  `contract_late_retraction_is_excluded_from_all_handover_exchanges`,
  `handover_excludes_child_prompts_and_text_and_reconciles_main_item_snapshots`.
- **Retry contract:**
  `partial_write_retry_preserves_buffer_and_never_duplicates_the_written_prefix`,
  `sync_failure_retries_sync_without_reappending_already_written_records`,
  `failed_writer_keeps_encoded_records_immutable_when_new_text_arrives`.
- **Checkpoint barrier:**
  `child_cache_checkpoint_excludes_later_appends_and_unrelated_actors`.
- **Cockpit behaviour:** `a_parked_thread_revives_with_its_history_and_says_so`,
  `a_park_then_revive_keeps_the_chosen_provider_and_model`,
  `a_parked_switch_waits_for_revive_and_a_revived_switch_still_carries`,
  `thread_title_is_durable_and_blank_rename_is_refused`,
  `a_thread_follows_the_worktree_its_main_creates`,
  `a_followed_worktree_that_vanishes_returns_the_thread_to_main`,
  `revive_group_is_ordered_and_rolls_back_when_a_later_member_fails`,
  `handover_commit_and_delivery_wait_for_readiness_and_failed_send_keeps_carry`.

**Guards that change their injection, not their behaviour.**

- `peek_reads_the_header_line_and_never_the_records` becomes "reads the
  header and a bounded tail".
- `failed_handover_rewrite_leaves_no_durable_handover_and_keeps_serving`
  injects its failure through the worker's `DurableWrite` seam instead of a
  `log.jsonl.tmp` directory.

**New tests, each written red first.**

- **Amendments:** an amendment leaves every earlier byte and the inode
  unchanged. Peek sees an amendment before the worker writes it. The fold
  equals each mark's restated facts. A handover alone amends the provider.
- **Tail repair:** truncation keeps the prefix byte-identical, without a
  second parse.
- **Upgrade:** it copies record bytes verbatim for each v1–v12 fixture, and
  keeps a damaged original.
- **Bounded revive:** it equals full replay as a property over seeded streams
  (`arbitrary_mid_turn_events`) with small `ActivityLimits` and a small mark
  spacing. Compared observables: Main blocks and runtime, children,
  `Revival` facts, resume target, recall, `first_prompt_sent`, owed
  exchanges. An env-gated `#[ignore]` lab test checks the same on the 127 MB
  copy.
- **Bounds:** revive and peek read bounded bytes, checked through a counting
  file seam.
- **Summary:** the parked summary equals full-replay counts, and
  `ParkedLookups` never replays a schema-13 log.
- **Worker:** amendments and records land in call order. A failing disk holds
  draining and loses nothing, and recovery resumes in order. Quit flushes
  every writer within the deadline. Park returns without waiting for a sync.
  Revive waits for a parked Thread's retained records. On Windows, delete
  waits for the worker to close the log.

**Commit order.** Each commit is independently shippable and green. The
review's blockers land before the first commit that changes anything on
disk:

1. Test-only: fixtures, invariant tests, the counting file seam, and the
   ignored lab tests.
2. **One writing process per store**: the store lock, read-only second
   instances with a UI notice, stores keyed by device and inode, and unique
   temp names.
3. **Sync levels**: a barrier at boundaries, F_FULLFSYNC at park, create
   and rewrite.
4. Repair by copy-then-truncate, parsing from the newest intact mark, which
   for now means from the start. `has_torn_tail`'s second full parse goes.
5. Byte-verbatim, crash-safe upgrade and replace: unique temp name, full
   sync, rename, directory sync or write-through. A fault-injected test
   covers every step.
6. Schema 13, `facts` deltas folded by the writer, facts-carrying
   `handover`, the commit path with truncation rollback, and marks that
   restate the facts. The setters and `hand_over` append, and `rewrite`
   survives only for the upgrade. `set_title` drops from ≈270 ms to
   ≈0.1 ms.
7. The mark summary, `ThreadMeta.summary`, and `LogReader` and
   `ParkedLookups` on the summary. The derived summary cache covers pre-13
   logs, and its entry carries over into the upgrade's mark. Parked
   `set_provider` and `delete` stop loading the whole log.
8. Base marks with carry. The upgrade computes a base, and `Store::revive`
   returns a `Revival`, with the full fallback. Revive goes from O(log) to
   O(retained).
9. The store worker: writer handles, health, backpressure, and barriers
   (checkpoint, which answers `Err` per serial; delete; revive), plus
   queued markers with generations.
10. Park and E1 quit through the worker, `log.pending-*`, and the panic
    hook.
11. Asynchronous hand-over commit.
12. The sync thread, including the 30 s full sync.
13. `queue.json`: a barrier before its rename, and a Notice when it is
    unreadable.

## Review (2026-10-07)

An independent review of the draft raised eleven findings. Each is resolved
above:

1. **Two processes on one store.** Resolved by the store lock and read-only
   second instances, in §1 and commit 2.
2. **Plain fsync cannot prove that only the last line tears.** Resolved by
   barrier syncs, repair from the newest intact mark, and copy-before-
   truncate, in §3 and §4.
3. **Full-restatement facts could undo an async handover.** Resolved by
   setters sending deltas that the worker folds, and a failed rollback
   setting `Failed`, in §1 and §2.
4. **Quit or a crash could drop accepted records.** Resolved by quit always
   waiting for writes, `log.pending-*`, and the panic hook, in §1.
5. **Existing logs would never get a bounded revive.** Resolved by bases at
   any offset, which the upgrade computes, in §3.
6. **Windows truncation, deletion, directory sync and copies.** Resolved in
   §5.
7. **Checkpoint on a failing writer.** Resolved by answering `Err` for that
   serial, in §1.
8. **A park removing a revive's `.open` marker.** Resolved by queued
   markers with generations, in §1.
9. **Children at the base, turn starts and property tests.** Resolved in
   §3.
10. **`queue.json` barrier and Notice.** Resolved in §4.
11. **Remaining full loads.**
    - `first_prompt_sent` is read from `summary.prompted`.
    - Parked `set_provider` and `delete` read the summary and the facts,
      in commit 7.
    - A parked pre-13 Thread keeps its summary across the upgrade that its
      first rename causes, in commit 7.
    - ADR 0002's "Ordinary Thread loading still reads a full snapshot" now
      means the bounded `Revival`.

## Consequences

The UI thread no longer performs file IO, except for bounded peek misses and
`queue.json`. Every operation that rewrote the log is gone.

- Revive cost and memory track the retained window, not the age of the
  Thread.
- Parked rows cost O(header).
- The log gains a derived index. It is pure acceleration: a reader that
  ignores marks gets the same answers more slowly.
- Upgrading costs one pass per old log, once, and is one-way for older
  Ferrites.
- Children older than the replay base start evicted after a revive.
- A park whose flush fails is reported afterwards rather than refused.

CONTEXT.md gains a **Header** entry.

## Decisions (operator, 2026-10-07)

- **Park** reports a failed write afterwards. It never blocks on a sync.
- **Revive** stays synchronous but bounded. An asynchronous revive is not
  planned.
- **Subagents** introduced before the replay base load lazily on first
  selection.
- **Pre-13 parked logs** use a derived summary cache file in the store
  directory. It is deletable, computed in the background, and never a full
  replay on the UI thread.
- **Upgrades** are lazy, and there is no downgrade tool.
- **Durability:** plain `fsync` at boundaries; F_FULLFSYNC at park, quit,
  upgrade and `create`, and every 30 s for logs written since. A 30 s
  power-loss window is accepted.

Every upgrade or rewrite writes a temp file, syncs it, renames it and syncs
the directory. None re-serializes an existing record. A fault-injected test
proves that a failure at each step loses no data.
