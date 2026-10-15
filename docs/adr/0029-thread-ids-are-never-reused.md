---
status: accepted
date: 2026-10-15
---

# Thread ids are never reused

## Context

The Store numbered a new Thread one past the highest Thread directory it
held. Deleting the newest Thread therefore handed its id to the next one.
State keyed by `ThreadId` outlives a deletion: the coordination book
(ADR 0028) keeps coordinator and worker ids, and an API caller waiting on or
subscribed to an id keeps it too. A new Thread that took a deleted
coordinator's id was treated as its Project's coordinator. One that took a
deleted worker's id received that worker's PR feedback and had its lessons
harvested.

## Decision

Thread ids are monotonic for the life of a Store directory. The Store keeps
a high-water mark, `next-id`, beside the Thread directories. To create a
Thread it takes the larger of the mark and one past the highest directory.
It writes the mark one past that with `replace_file`, and only then creates
the directory. The mark never goes down, so a crash may skip an id but
never hands one out twice. Minting holds a lock in the process's shared
Store state, and only the holder of the Store's claim (ADR 0009) may mint.

A store from before the mark has no `next-id`; its first creation seeds the
mark from the Threads it holds.

The fix belongs to the Store rather than to each keyed consumer, so every
consumer, present or future, can trust an id to name one Thread only.

## Consequences

- An id held after its Thread is deleted names no Thread. It never names a
  stranger.
- One more small file is written, with one more sync, on each Thread
  creation. Creation is rare, so the cost does not matter.
- Deleting `next-id` by hand can only bring back ids whose Threads are
  already gone. Its loss would allow reuse again, as before this ADR.
