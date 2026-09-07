# Live UI lag investigation — 2026-09-06

Ferrite's group view saturates its main thread preparing frames. Window-wide
text-selection registration does quadratic work, transcript layout includes
offscreen content, and composer edits invalidate the entire cockpit. The
operator's fullscreen comparison sharply reduces the measured cost while the
same app process and its provider sessions remain running.

This report records the original investigation before implementation. That
stage changed no application source, installed binary, live settings, or thread
state. Small diagnostic binaries ran separately, headless. The subsequent
architecture and its validation are recorded in [ADR 0006](../../adr/0006-retained-transcript-rendering.md)
and [implementation validation](validation.md). The running app was never
restarted or replaced during either stage.

Source line numbers below refer to the original investigation checkout.

## Live evidence

Target: `/Applications/Ferrite.app/Contents/MacOS/ferrite`, PID **12907**, launched
20:25:20 ACST. First profile began at **20:54:39**, during the reported lag.
Checkout: `d8413ed`. Installed and cached release binaries share Mach-O UUID
`E3F63F67-F856-3771-96A5-484A6508A008`. The binary does not identify its Git commit;
matching function names and the contemporaneous checkout support source mapping.

Six passive macOS `sample` captures, 5–10 seconds each, were taken without
relaunching the app. Percentages below count each main-thread stack once for
the named operation. Rows overlap: selection and layout are inside drawing.

| Measurement | Group view | Confirmed fullscreen pane |
| --- | ---: | ---: |
| Ferrite CPU | Approximately one busy core in repeated `ps` snapshots; interval `top` readings 73–93% after initial system contention eased | 46.3%, 51.4% in two `top` intervals |
| Main-thread samples inside `Window::draw` | 96.2–96.8% in four captures; 89.3% in a later group capture | 50.8% |
| Direct selection registration | 22.5–32.0% | 3.1% |
| Taffy layout computation | 24.9–31.0% in the first four captures | 20.6% |

The operator confirmed the group was still showing at 21:09, then explicitly
confirmed fullscreen before the **21:12:29** capture. The group sample named
`sample-after-fullscreen-note.txt` is therefore **group**, despite its filename.

System load is a contributor, not a sufficient explanation. Initially concurrent
Rust builds saturated nearly all CPU capacity. Later, Ferrite still consumed
92.8% CPU with **52.5% system CPU idle**. During the smooth fullscreen comparison,
the machine was busier again: only **2–6% CPU idle**. These are live observations,
not a controlled machine-wide benchmark; streaming content and system load kept
changing between captures.

Ferrite's reported footprint grew during ongoing sessions, from about 269 MB
initially to 423 MB in the final `top` capture. No allocation profile was taken,
so this does not establish a leak. Interval reports did not show sustained swap
activity sufficient to explain the persistent group-only symptom.

## 1. Selection registration is quadratic across the window

In [text_selection.rs](../../../vendor/gpui-base/src/text_selection.rs),
`WindowSelectionState::register_participant` (line 992) does this for **each**
registered text participant:

1. Scan all existing participants and upgrade their weak entity handles.
2. Insert/update the current participant.
3. Call `publish_snapshots` (line 1384), which scans for dead participants again
   and visits every participant to compute/update its selection snapshot.

The unchanged-snapshot guard in `SelectableTextState::set_snapshot` (line 621)
prevents redundant events, but only after the global traversal and entity update
have happened. No active text selection is required to pay this cost.

The registry belongs to the **window**, not to each pane. Scope filtering happens
inside snapshot publication, so inactive panes still contribute scan work. If
there are N retained participants, a steady frame performs roughly N registrations
over N participants: **O(N²)**. Doubling equal-sized panes can therefore multiply
this part of the work by approximately four, not two.

The live stacks corroborate the source: `TextView::paint → TextSelectionHandle::register
→ WindowSelectionState::register_participant → publish_snapshots`, with many samples
in weak-handle upgrade/drop and the registry's `HashMap::retain` traversal.

An early broad symbol search overestimated selection time by including the
generic `TextSelectionScopeMarker` wrapping child rendering. The final numbers
use exact demangled function names and exclude that wrapper's unrelated work.

## 2. Offscreen transcript content still costs layout

[pane.rs](../../../crates/ferrite/src/pane.rs), `rendered_window` (line 2190), takes
the last **200 transcript blocks** at Transcript level, as configured in
[docview.rs](../../../crates/ferrite-core/src/docview.rs), line 227. `body` then
constructs children for that entire retained tail in a scrolling flex container.
It is a history cap, not viewport virtualization.

[rich.rs](../../../crates/ferrite/src/rich.rs) retains parsed native documents,
which avoids parsing unchanged Markdown again. Its natural-height `TextView`
elements still participate in layout and paint traversal. A block is not the
same as a selection participant: answers can coalesce, and other blocks can
produce several text elements.

The existing clipping patch skips per-character hitbox calculations for fully
clipped inline text. It does not remove the enclosing document from layout or
prevent its selection adapter from registering. `TextView::paint`, lines 712–729
of [text_view.rs](../../../vendor/gpui-base/src/text/text_view.rs), registers
selectable views without a corresponding whole-document visibility guard.

The live layout samples and fixed-viewport differential probe both show the
remaining cost. Reducing only selection work will leave substantial layout work.

## 3. Typing and streaming invalidate the cockpit

[cockpit.rs](../../../crates/ferrite/src/cockpit.rs), `composer_edited` (line 681),
ends with `cx.notify()` on the cockpit. The stream pump (line 751) also notifies
that entity when it observes changes. Pane rendering is assembled through
`pane_cell` and the group tree within the cockpit's `Render` implementation.

Consequently, editing a prompt can rebuild transcript presentation across the
group, even though most transcript content did not change. This connects the
measured frame-preparation work directly to the operator's typing symptom.
The pump does have an unchanged-state early return; there is no evidence here
that its 8 ms timer alone unconditionally redraws an idle cockpit.

Fullscreen is particularly informative. Around line 5030, the renderer deliberately
omits sibling panes from layout while their sessions continue through the pump.
That reduces both layout size and the population of the window's selection
registry after frame cleanup. Wider text and different wrapping can also help;
the live comparison does not isolate those effects from the participant count.

## Reproduction and measurements

The probes link to the actual compiled GPUI Kit/Base **debug test-support**
libraries. They do not imitate the selection algorithm. They exercise the native
registration API and native `TextView` rendering in headless windows, respectively.
The registered participants retain stable identities between measured frames.

**These milliseconds are debug-harness CPU preparation costs, not release-app
frame times, input latency, or measured display FPS.** The scaling and differential
comparisons are the relevant evidence.

Selection-only baseline, with essentially no text-layout workload:

| Participants | Median frame preparation |
| ---: | ---: |
| 0 | 0.045 ms |
| 64 | 12.208 ms |
| 128 | 51.966 ms |
| 256 | 202.107 ms |
| 512 | 794.697 ms |

Four times as many participants (64 → 256) cost **16.56×** as much. Doubling
128 → 256 and 256 → 512 cost approximately **3.9×** each.

Native Markdown documents in a fixed 900 × 600 scrolling viewport:

| Documents | Selection disabled for the experiment | Selection enabled |
| ---: | ---: | ---: |
| 32 | 10.117 ms | 15.241 ms |
| 128 | 43.847 ms | 112.281 ms |
| 200 | 80.739 ms | 185.288 ms |

This is an experimental control, not a proposal to remove selection from Ferrite.
At 200 documents it cuts measured preparation by about **56%**, while the
remaining 80.7 ms demonstrates additional work beyond selection.

The regression checker was run against the separately compiled probe:

```sh
python3 docs/research/ui-lag-2026-09-06/check-selection.py \
  /tmp/ferrite-lag-20260906/probe/selection_probe
```

```text
4x participants: 7.27x cost; linear budget: 6.00x
FAIL: selection registration exceeds the linear scaling budget
```

Exit status **1**, intentionally red. That repeat encountered substantially more
system contention than the first baseline; its absolute times and ratio varied,
but the scaling guard still failed. This is a diagnostic timing guard, not a
deterministic CI regression test or an end-to-end typing test.

Probe sources, full timing logs, [profile-summary.json](profile-summary.json), and
[summarize-samples.py](summarize-samples.py) are retained here. Raw profiles and
system captures are in [live-captures.tar.gz](live-captures.tar.gz); working copies
and the already-built probe executables remain in `/tmp/ferrite-lag-20260906/`.

Original build invocation (repeat for `transcript_probe.rs`):

```sh
rustc --edition=2021 --test selection_probe.rs -o /tmp/selection_probe \
  --extern gpui=/path/to/debug/deps/libgpui_kit-HASH.rlib \
  -L dependency=/path/to/debug/deps -C debuginfo=0
```

Use the test-support library from a build of the checkout being measured. The
shared Cargo cache was being rebuilt by other work during this investigation;
the original library hash disappeared after the probes were linked. Do not
compare a patched checkout using an old cached probe executable.

## Why the original Metal spike did not catch this

[spikes/panes24](../../../spikes/panes24/src/main.rs) renders **14 simple text
lines per pane**. The current app mounts rich selectable documents, native
controls, and a much larger retained transcript tree. The spike measured an
earlier, lighter workload; it did not cover the selection registry introduced
with native rich text or realistic group history.

Metal submits the finished scene. CPU layout, document preparation, selection,
and event handling must finish first. The profiles directly establish expensive
main-thread preparation. They do not measure GPU utilization. CPU-side samples
inside `MetalRenderer::draw` were only a small fraction of the captures.

## Fix priorities and acceptance criteria

1. **Remove per-participant global sweeps/publication.** Maintain geometry in O(1)
   per registration and batch lifecycle work once per frame; provide a cheap path
   when no window selection exists. Preserve selection across blocks, scope
   isolation, copy, dead/stale participants, resizing, and drag auto-scroll.
   Active-selection geometry ordering needs explicit tests before changing when
   snapshots become visible during paint.
2. **Virtualize transcript layout to the viewport plus overscan.** Keep durable
   text/selection identity and access to full copyable content while removing
   offscreen rows from repeated layout. The current 200-block tail and parser
   cache do not provide this bound.
3. **Separate composer and pane invalidation.** Keep a keystroke from rebuilding
   every sibling transcript. Isolate pane/view rendering and update only content
   whose revision or display state changed.
4. **Benchmark the production workload.** Use long real-shaped transcripts,
   several Transcript-level panes, ongoing streaming, and concurrent typing.
   Measure release draw-duration and key-to-paint distributions against 8.33 ms
   at 120 Hz or 16.67 ms at 60 Hz. Cover group → fullscreen → group transitions
   and test with and without active selection.

## Limits

Actual display FPS and key-to-paint latency were not captured. macOS reported
screen-capture access unavailable, and UI-control attempts did not complete.
The existing `FERRITE_PERF` counter is enabled when the view is constructed, and
this app's stderr is `/dev/null`; enabling it would require changes outside this
no-restart investigation. A statistical sample cannot recover exact frame rates.

Fullscreen is a confirmed workaround in this session. No fix has been installed
or measured against the running group. This report establishes the original
bottlenecks; [implementation validation](validation.md) describes the separate
worktree changes and their measured limits.
