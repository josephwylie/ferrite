---
status: accepted
date: 2026-10-07
---

# Retained history trims in place

## Context

Each Subject's displayed history is bounded (4 MiB of content, 2000
Blocks). The Activity keeps the records it folded so that a retraction,
a snapshot or an alias can replay them. At the cap, every append trimmed
the oldest record and replayed every remaining one into a new
Transcript: a new history generation per streamed delta. Each generation
re-keyed the Pane's text namespace, resynced the whole transcript, built
new native text, and left the old namespace's ⌘-click route, link boxes
and cached text behind. Replaying a real 127 MB Claude log through the
Activity, the path a parked Thread's counts take, rebuilt history 7385
times in 34 s.

Claude reports a progress phase before every thinking delta. These
reports split each thought into one record per delta, so trimming the
oldest records split a thought on almost every append.

## Decision

Trimming evicts in place. Each record notes the newest Block it wrote.
Each Block notes the last record that gave it content. Trimming records
drops their Blocks from the front of the Transcript, as its own Block
limit already does. Ids, Markdown runs and the history generation stay
unchanged. A replay is kept only where it changes what remains: a
surviving record streamed into a trimmed Block, or found one it repeated,
or a single event exceeds the limit. A property test holds the result
equal to a rebuild from the retained records.

A streamed item stays one record across reports that write no Block
(progress, usage), and a report repeating the last record is not recorded
again. Neither changes what a replay draws.

What a namespace held goes with it. A transcript owns the ⌘-click route
registered under its namespace. A new namespace, or the transcript's
release, forgets that route, its link boxes and its native text. A
Subject switch keeps both transcripts and their namespaces.

A Pane's native text cache charges each text what it holds: about 32 KiB
plus 144 bytes per source byte, as measured by the memory freed when
entries dropped. The cache keeps at most 32 MiB of that, and 256 entries.
The 64 most recent lookups are never evicted for another, so texts drawn
together stay cached.

## Consequences

The same log replays in 0.6 s with 2 rebuilds. Twenty appends at the cap
keep one namespace. Six regenerations leave one route and one scope of
link boxes, not seven and seven.

The visible history limit is unchanged; this changes how history is
trimmed, not how much is kept. The provider log stays the durable
history: resume ids, handover exchanges and every event are read from
disk, never from the trimmed projection. As in
[ADR 0006](0006-retained-transcript-rendering.md), an eviction still
invalidates a selection that reached the evicted Blocks.

A record's Block attribution must account for every way a later input can
depend on an earlier Block. An input that only settles a Block (a tool's
result, a prompt's time) is not an author. A replay without the Block
finds nothing to settle, so evicting the Block matches it. A new
dependency of that kind needs its attribution, or the property test
fails.
