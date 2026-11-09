# First full work-order execution: final observation gate red

Source `f0526595aeed0f9847dd7607065c15e62ebe172c`.
Run https://github.com/josephwylie/ferrite/actions/runs/42572976607
Artifact contains the complete raw logs; this folder retains the task's exact
public tool-response trace, peer/window result, provenance and independent
fixture records/audit read after app exit.

The actual ClaudeSession drives the production bound browser tools through
create, field fills, native text typing, dropdowns, checkboxes, dynamic fields,
two saves, filtering, reopening and later edits. All 41 steps in this initial
plan complete before final observation validation. The independent fixture
[record and audit](work-order-oracle-at-exit.json) have every expected final value
and exactly two successful saves for the task. The final [readable observation](work-order-trace.json)
also contains those field values and checked/selected state.

The final gate fails: the product's readable snapshot rewrites plain
`[ref=eN]` to opaque IDs but leaves native IDs in attribute-bearing brackets
such as `[required, ref=e64]` and `[checked=false, ref=e59]`. The offered refs map
uses opaque keys, so the printed references cannot be used or linked reliably.
The harness also needed to read actual `checked=true/false` and comma-separated
selected attributes. The targeted product and parser fixes require a new actual
run; this is not a passing task and its duration is not included in performance
percentiles.

This initial plan contains 30 tool action/navigation steps and eleven readiness
waits. Setup navigation was via Preview and Description focus was implicit in
Type. The following revision adds explicit provider Navigate and Description
Click to preserve the prescribed 32 logical steps. Consequently these first
scenario durations are not comparable warm benchmarks. No controlled 1280×713
viewport, partly-visible recovery, sustained-resource or full release claim is
made. The task trace reports no viewport recovery counters in this viewport.
