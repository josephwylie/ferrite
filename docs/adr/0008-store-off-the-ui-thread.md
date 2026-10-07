---
status: proposed
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
| `set_title`, `set_workspace`, `set_provider` | Returns at once. Appends a `facts` amendment through the same FIFO, so no `writer` parameter is needed and it is removed. The meta cache that `peek` reads is updated before returning. |
| `peek` | Served from the in-process meta cache. The worker warms it for every Thread at open. A miss reads the header plus the tail back to the newest facts-bearing record, which section 3 bounds. A miss never waits on the worker: an amended Thread is always cached. |
| `ThreadWriter::health()` (new) | Answers `Ok`, `Backlogged` or `Failed(message)` without blocking. |
| child-history checkpoint | Asynchronous. A `Checkpoint` command writes the Thread's buffered records, then forwards the ADR-0002 request to the reader with `through` = committed length. The cockpit inserts its `Pending` buffer when it enqueues. Everything accepted after the barrier lands past `through` and replays once from the buffer, the same contract as today. |
| `park` | Hands the writer to the worker, which writes, appends a mark, F_FULLFSYNCs and removes `.open`. Park no longer waits. A failure is retained and reported (below). |
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
`Store::close_all(QUIT_DEADLINE)`. That writes every buffered record and a
mark, then requests F_FULLFSYNC. It waits for the writes and for as much of
the sync as fits in the deadline (proposed 500 ms), and never longer. Data
that has passed `write(2)` survives process exit. Only power loss inside the
deadline can cost the unsynced tail.

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
order:

- `facts` replaces all the facts.
- `handover` sets `provider` to its `to`, `model` to its model, and `effort`
  to `None`.
- `mark` changes nothing. It restates the fold up to itself, and a test pins
  that equality.

`load` and `peek` share one implementation of the rule.

**`peek`.** On a cache hit, peek is a memory read. On a miss it reads line 1
for the schema. Below 13, line 1 is the answer. Otherwise it scans back from
EOF for the newest `facts`, `mark` or `handover` record. A handover keeps the
scan going until the next facts-bearing record, then applies on top of it.
The mark policy (section 3) bounds this scan to about 1 MiB plus one flush
batch, which is never the 12–25 MB measured above. `peek_first_prompt` is
unchanged.

**`hand_over`.** The new provider and its Handover must commit together. One
line already does that: the `handover` record now implies the facts change,
so no second record is needed. The worker runs the commit in three steps:

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

**When the worker writes a mark.**

- **Policy:** at the first flush boundary after `max(1 MiB, 16 × last mark
  size)` bytes since the previous mark, and always at park, quit and upgrade.
  That keeps the overhead below 6.25% and bounds the tail that `peek` and the
  summary must scan.
- **Turn starts:** a mark written immediately before a Main `prompt` record
  carries `base`, which makes it a replay base.

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
2. Find the newest mark by reverse scan, then follow `prev` back to the
   newest base mark B where `cost(newest) − cost(B) ≥ 2 ×` the Main content
   budget (8 MB). If no mark is that far back, B is the start of the log.
3. Read the records named in B's `carry` by offset, then read `[B, EOF)`
   sequentially.
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
| (c) child identity | Per canonical child: its first introduction, every alias, and its newest status and outcome |
| (d) recall | Every `prompt` since the last reset |

**Why the replay is equivalent.** The Main window is a suffix, and trimming
only pops from the front. So any replay that contains every record from a
turn start at or before the window start ends with the same retained Main
records. `rebuild` keeps the runtime that the carried records set.

Children introduced before B are restored with identity, status and outcome
intact, then evicted. Selecting one reloads its content through the
ADR-0002 loader, which reads from disk off the paint path. With no usable
mark (a crash before the first mark, or a carry offset that fails to parse
as its declared kind), revive falls back to a full replay. Correctness never
depends on the accelerator.

Persisting Activity's derived runtime in the mark was rejected. It would
make internal state, which changes freely, part of the schema. Carry offsets
name records the log already holds.

**Torn tail without a second parse.** A schema-13 log can only tear at its
last line: writers append whole lines, a failed write finishes its own bytes
on retry, and every open repairs first. Repair reads the last 64 KiB. If the
file does not end in `\n`, or its last line does not parse, the worker
truncates to the end of the last good line with `ftruncate`, then syncs.
Nothing is rewritten.

A mark certifies that every line before it parsed when it was written. That
is what lets the bounded reader keep `load`'s "the first unreadable line is
the end" meaning without reading the prefix. Corruption of a pre-13 log in
the middle is found, once, by the upgrade's full parse.

**ADR-0002 caches.** Byte offsets no longer move after an amendment, so the
`history.clear()` invalidations in `apply_move`, `rename_thread`, `set_model`
and `set_effort` become unnecessary. They stay in place until the tests prove
that is safe.

### 4. Sync policy and durability contract

| Moment | Sync |
|---|---|
| Boundaries: turn end, `closed`, `handover`, child terminal facts, amendments, the 5 s interval | Plain `fsync` |
| Park, quit, upgrade, `create` | F_FULLFSYNC |
| Any log written since its last full sync | F_FULLFSYNC every 30 s (proposed) |

All syncs run on the sync thread. On Windows both levels are
`FlushFileBuffers`.

The durability contract this implies: a record past a boundary survives a
Ferrite crash or a kernel panic. After power loss, everything up to the last
full sync survives. In the worst case, a sudden power cut loses up to 30 s of
records. The log stays consistent in every case, because tail repair handles
the cut.

`queue.json` keeps its synchronous save, with its refusal contract, but uses
plain `fsync`.

### 5. Migration and rollback

**Unchanged behaviour.**

- Pre-13 logs load unchanged and are never written until a revive or
  amendment upgrades them.
- An upgraded log keeps every record's bytes.
- Torn-tail repair removes only bytes that never formed a committed record.

**Interior damage.** The upgrade's full parse can find an unreadable line
that is not the last one. Today's `writer()` silently discards everything
after such a line. The proposal is to hard-link the original as
`log.damaged-<offset>.jsonl` before renaming, so no byte is lost.

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

**Commit order.** Each commit is independently shippable and green:

1. Test-only: the equivalence harness, the counting file seam, and the
   ignored lab test.
2. Torn-tail repair truncates the last line, and `has_torn_tail`'s full
   reparse goes. `writer()` drops from ≈260 to ≈135 ms, and revive goes from
   three full parses to two.
3. Sync levels: plain `fsync` at boundaries, F_FULLFSYNC at park, quit,
   create and rewrite.
4. Byte-verbatim upgrade replaces the reserializing rewrite.
5. Schema 13 and `facts` amendments. The setters and `hand_over` append,
   their `writer` parameters go, and `rewrite` is deleted except for the
   upgrade. `set_title` drops from ≈270 ms to ≈0.1 ms.
6. Marks with facts and summary, `ThreadMeta.summary`, and `LogReader` and
   `ParkedLookups` on the summary. This removes the launch-time parked
   replays.
7. Base marks with carry, plus `Store::revive` → `Revival` and the full
   fallback. Revive takes one bounded read and goes from O(log) to
   O(retained): ≈0.4 s of reads plus 32–42 s of replay becomes about 45 ms
   on the lab log.
8. The store worker, with writer handles, health, backpressure and barriers
   (checkpoint, delete, revive), one per directory.
9. Park and E1 quit through the worker.
10. Asynchronous hand-over commit.
11. The sync thread.

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

When this ADR is accepted, CONTEXT.md's **Settings** entry ("a Thread's
header remains the durable truth") gains a **Header** entry. A Header is the
Thread's facts as created, amended by appended records, with the newest
winning.

## Open questions

- **Park:** Report a failed write afterwards (proposed), or block on the
  write barrier, as park does today (write plus F_FULLFSYNC)?
- **Revive:** Keep revive synchronous but bounded (proposed for now), or
  make it asynchronous with a "reviving" Pane? The asynchronous version
  removes the remaining one-time upgrade wait of about 250 ms per old log.
- **Children:** Is it acceptable that children introduced before the replay
  base start evicted and reload on first selection?
- **Pre-13 parked summaries:** Use a derived summary cache file (proposed),
  or show no count until the Thread is revived?
- **Downgrade:** Upgrade lazily (proposed) or eagerly? Do we need a downgrade
  tool, given the effect on an older `Groups::load`?
- **Durability:** Is a 30 s periodic F_FULLFSYNC window acceptable?
