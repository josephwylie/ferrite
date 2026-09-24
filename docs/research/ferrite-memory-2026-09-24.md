# Ferrite memory investigation — 2026-09-24

The confirmed transcript-revision leak is already fixed by PR #112. This
investigation independently reproduced that leak with the fix removed, verified
the merged fix, and found and fixed a second retention leak in subagent tool
timings. The new fix and seven regression tests target both branches. Final validation
uses `main` based on `b0d879a` and `feat/ui-overhaul` based on `ab19485`. Main was
fast-forwarded from `344c7d7` before the final review and test run.

No Vitest ran. Validation uses Rust tests with synthetic provider events and
GPUI's native test platform. No provider prompts were sent, and no application
binary was installed or restarted.

## Transcript revisions: existing fix verified

Previously, `TranscriptView::sync_members` constructed discarded UI elements
outside a draw. GPUI's fallback arena retained those elements, their text, and
native-cache handles. In an isolated source snapshot, restoring that call made
`streaming_without_drawing_releases_temporary_transcript_elements` fail:
100 updates increased retained handles from **3 to 103**. Restoring the merged
plain-text collection path passed the same test. The UI overhaul also contains
this fix and passes its streaming regression.

## Evicted subagent timings: new fix

`SubjectState::bookkeeping` inserted a completed timing whenever a provider
reported a duration. `append` then returned early for an unretained Subject,
skipping pruning. Codex command results can supply these durations. Repeated tool
completions therefore accumulated metadata even while the child transcript was
empty. Subjects outside the transcript-cache budget were affected too.

The original reproduction used a 16-block budget and completed 1,000 tools while
retained, then evicted the transcript and completed another 3,000 tools:

| Point | Before fix | After fix |
| --- | ---: | ---: |
| Retained, 1,000 completed tools | 16 timings | 16 timings |
| Evicted, 1,000 more completed tools | 1,016 | 0 |
| Evicted, 2,000 more completed tools | 2,016 | 0 |
| Evicted, 3,000 more completed tools | 3,016 | 0 |

The fix gives completed timings the same lifetime as retained transcript
history. Eviction discards completed entries and shrinks their table; live tool
clocks survive until completion. Completions, turn endings and disconnections
release the remaining clocks for unretained Subjects. Historical timing restores
populate only retained Subjects. Durable events and live status still advance.

The original four tests in `crates/ferrite-core/tests/activity_memory.rs` failed
on both branches before the patch and pass afterward. They exercise automatic
eviction by selecting another child, never-retained children, in-flight tools,
native and locally measured durations, turn/session endings, and history restore.
A fifth Activity test verifies that rejected, duplicate, stale-generation,
historical and clockless completions do not publish invented durations. The
original 3,000-completion probe also passes after the fix.

## Review and preservation of tool durations

Claude Opus 5.5 reviewed the patch through separate standards and behavior
passes. Both identified a real regression in the first patch: Cockpit read the
completed duration from the cache after Activity had removed it. That would
lose durations from durable history for tools finishing while unretained.

Activity now returns the accepted live completion's duration directly in its
update. Cockpit persists that value independently of the transcript cache. A
public Cockpit regression verifies that native durations from never-retained
and evicted children, and local clocks started before eviction, survive reload.
It failed before this correction and passes afterward.

A related race needed the same treatment: a tool can finish after a child
history read freezes its disk checkpoint. The existing bounded history buffer
now carries those completion durations alongside its accepted events. They
restore before the older disk timings. A deterministic gated-reader test covers
locally measured clocks, newer native durations overriding a frozen older
value, and a second ordinary disk reload. It failed before the correction and
passes on both branches. No unbounded timing collection was introduced.

The second Opus 5.5 review found no blocking issues. The standards pass suggested
updating ADR 0002's list of Activity outputs; that wording now includes the
completion duration. The behavior pass noted one low-severity tradeoff, retained
intentionally: ending an unretained Subject's turn or Session releases its
remaining clocks. A later completion without a provider-native duration therefore
has no local timing to persist; a retained Subject can still carry the approximate
clock frozen at turn end. This release policy is covered by a regression test.
Provider-supplied durations remain available on accepted late completions.

## Validation

| Suite | Main | UI overhaul |
| --- | ---: | ---: |
| Activity, memory regressions, subagent history | 45 passed | 45 passed |
| Headless Cockpit, history and lifecycle | 107 passed, 1 ignored | 109 passed, 1 ignored |
| Activity store and replay | 19 passed | 19 passed |
| Native subagent UI | 14 passed | 14 passed |
| Transcript rendering | 11 passed, 1 failed, 1 ignored | 20 passed, 1 ignored |

Final totals: **196 passed, 1 failed, 2 ignored on main**; **207 passed,
2 ignored on UI overhaul**. These runs include the final persistence and
history-race corrections.

The main rendering failure is
`retained_transcript_relative_file_links_use_the_thread_workspace_and_copy_text`:
selection returns a suffix ending in `next s` instead of `next steps`. It also
fails in the unchanged `344c7d7` baseline and is separate from this timing fix.
The UI overhaul's corresponding test passes. The ignored tests are opt-in
thread-revival and streaming-layout performance probes.

UI overhaul validation uses its separate
`/Users/josephwylie/.cache/uio-foundation-target` build directory. Initial attempts
to share main's build cache picked up stale vendored GPUI APIs and are excluded
from the results above. Formatting of the new tests and `git diff --check` pass
in both working trees.

The suite commands, run from each working tree with its build directory, are:

```sh
cargo test --locked -p ferrite-core --test activity --test activity_memory --test subagent_cockpit
cargo test --locked -p ferrite-core --lib cockpit::
cargo test --locked -p ferrite-core --lib store::activity_tests::
cargo test --locked -p ferrite render_performance::
cargo test --locked -p ferrite cockpit::tests::subagents::
```

Raw commands, source snapshots and logs are under
`/Users/josephwylie/.cache/ferrite-memory-audit-20260924-_veii24y`.
The final commands and results are in `main-final-checks.json` and
`ui-final-checks.json`; review responses and exact model metadata are in
`opus-review/`. Earlier evidence includes `fix-verification.json`,
`baseline-verification.json`, `main-timings-before.log`,
`ui-overhaul-timings-before.log`, and `original-timing-probe-after-fix.log`.
The duration regressions have separate `duration-persistence-before.log`,
`duration-persistence-after.log`, `duration-history-race-before.log`, and
`duration-history-race-after.log` records.

The September 18 live process measured 7.8 GB of physical footprint, mostly
swapped out. A different September 24 UI overhaul process measured approximately
456–529 MB, with a 939 MB peak. Those are different builds and workloads, not a
controlled before/after memory comparison. The allocation-lifetime tests support
the two specific diagnoses; they do not establish that every possible Ferrite
workflow is free of leaks.
