# 0006 — Retained transcript rendering

Status: accepted 2026-09-06; implemented and validated in the isolated worktree
2026-09-07.
See [validation](../research/ui-lag-2026-09-06/validation.md). Not installed into
the running app during this investigation.

## Context

The running macOS app lagged in a Group and recovered when one Pane was
fullscreen. Passive samples put 96–97% of its main thread in drawing, with
substantial time in text-selection registration and layout. Metal draws the
result; it does not eliminate the CPU work that constructs that result.
See [captures and reproduction](../research/ui-lag-2026-09-06/README.md).

The Cockpit reconstructed every visible transcript after a Composer edit or
an event in any Thread. Cached native parsers avoided parsing the same text,
but their elements still underwent layout. Each native selection registration
also walked the window's participants. Registration therefore grew
quadratically with the number of mounted text views.

## Decision

Retain an independent transcript Entity for each Subject. The Cockpit owns
provider events, focus, Pane chrome and Composer actions. The transcript owns
its row snapshot, native lazy list and selection document. Its input changes
when that Subject's content or presentation changes. A central Cockpit observer
compares those keys before drawing and updates only changed transcript entities.
Rendering repeats the cheap comparison for initial mount and derived focus
state. This ordering makes child invalidation visible to the current frame.
A notification elsewhere in the Cockpit can reuse its cached rendering.

Presentation revisions include ordinary stream mutations and restored tool
timings. They are separate from history-rebuild generations, which continue to
name stable native text identities. Live progress remains a Pane sibling, so
its elapsed-time updates and pulse do not invalidate the cached transcript.

Project retained history into stable semantic rows. Reconcile insertions,
evictions and changed contents against GPUI's variable-height `ListState`;
lay out the viewport and a small overdraw region. Native list state is the
single authority for scrolling and following the live tail. Subject changes
retain independent view state.

Separate native selection's logical membership from visible geometry. A
Subject's ordered text identities and copy callbacks persist when rows leave
the viewport. Visible rows supply geometry; cached views replay only that
visible geometry. Registration does constant work per participant and the
window publishes projections once after the frame. Pointer actions publish
immediately. Actual history eviction or scope changes invalidate selection;
ordinary scrolling does not.

Logical members hold weak references to native views. Only selection endpoints
pin their native state beyond the existing text cache. Recreated views match
endpoints by logical member key. Native endpoints carry inline and UTF-8
positions, so reflow and recreation preserve the selected characters. Paint
and copy use the same normalized logical range. Exact source versions invalidate
endpoints when their selected fragment changes; updates to other fragments
preserve selection. This avoids an unbounded second parser cache.

Inline positions identify the original parser run, not its wrapped visual
fragments. Text and custom file-link cards retain byte offsets into that run;
fragment-to-source references are weak. Reflow can split the visible text
differently without changing the logical selection or copied link label.

Core Subject eviction releases its retained transcript entity, even when that
Subject is hidden. Ordinary Subject switches preserve the entity and scroll
position. Subscription bookkeeping also follows the remaining entity owners.

Ferrite derives membership from the same presentation functions used for
visible rows, without mounting or laying out the resulting elements. This
pass runs on content/disclosure changes. It avoids a second model of which
tool details, reasoning or literal fragments are selectable. Native GPUI
continues to own hit testing, highlighting and copy projection.

## Consequences

Typing and streaming have explicit regression tests requiring zero native
text renders in unaffected sibling transcripts after initial parsing and input
modality settle. GPUI intentionally refreshes all hover/focus-visible styles on
a mouse-to-keyboard transition. A fixed viewport must mount
a bounded number of rows as retained history grows. Selection tests cover
copying across offscreen rows and removal of real logical members.

The existing retained-history limit remains a separate product decision.
Lazy row measurement makes the scrollbar's global pixel extent approximate
until rows have been measured; the native logical scroll anchor preserves
reading position. A single semantic Markdown answer still uses its native
document layout and can be taller than the viewport. These changes remove
cross-Pane reconstruction and whole-history row layout; they do not claim a
constant layout cost for arbitrarily large individual documents.

This extends [ADR 0003](0003-native-gpui-components.md): its render-window
eviction rule concerns retained logical history, not viewport mounting. No
second Ferrite selection engine or provider execution model is introduced.
