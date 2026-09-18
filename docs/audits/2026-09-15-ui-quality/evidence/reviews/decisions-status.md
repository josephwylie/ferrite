# Ferrite UI quality — native Group, Question, status fixtures

Current source: `/Users/josephwylie/Desktop/Projects/ferrite/.worktrees/ux-audit-2026-09-14`. Only a disposable copy changed at `/tmp/ferrite-ui-decisions-fpg0q4yc`. No product source changes, real provider requests, user store reads, or live GUI control.

## Native captures and reproduction

Final screenshots: `/tmp/ferrite-ui-fixtures/`

| File | Logical viewport | Surface |
|---|---|---|
| `group-l1-1200x800.png` | 1200×800 | Four Threads, two working, one completed, one failed |
| `group-l2-860x500.png` | 860×500 | Same ordinary Group in Instruments mode |
| `question-group-1200x800.png` | 1200×800 | Same Group, normal two-option Question |
| `draft-1000x800.png` | 1000×800 | Empty draft with a project registered |
| `new-project-1000x800.png` | 1000×800 | First run, empty store, native New Project editor |
| `subagents-1000x800.png` | 1000×800 | Main with three Subagent tabs |

PNGs are native 2× framebuffer captures. `manifest.json` records pixel dimensions. The first-pass screenshots are retained under `initial/` for provenance, but their overlapping in-flight toasts are an immediate-capture timing artifact and are **not counted as a defect**. The final pass clears transient notices through the existing core API before rendering, matching the state after the user clears the bell; rendering code is untouched.

Exact final command, run in `/tmp/ferrite-ui-decisions-fpg0q4yc`:

```sh
CARGO_TARGET_DIR=/Users/josephwylie/Desktop/Projects/ferrite/target cargo run -p ferrite --features visual-reference -- --visual-reference /tmp/ferrite-ui-fixtures 2>&1 | tee /tmp/ferrite-ui-capture-final.log
```

Result: compilation succeeded in 4.63 seconds, six `CAPTURE` lines, all six PNGs inspected or handed to the owning audit agent. Initial build/capture succeeded in 10.11 seconds. Full logs: `/tmp/ferrite-ui-capture.log`, `/tmp/ferrite-ui-capture-final.log`. Complete disposable harness diff: `/tmp/ferrite-ui-capture.patch`. No new test suite was created for this extension.

## Visual findings

### VQ1 — P1 major: An ordinary Question escapes its Pane and hides its identity

**Evidence:** `question-group-1200x800.png` (confirmed in both passes).

**What is actually visible:** The top-left Pane's two-option Question starts above its header, hiding the Thread title and running into the application breadcrumb/titlebar. It also covers essentially all of the relevant transcript context. The prompt is an ordinary sentence and each option has a short description; this is a normal four-Thread desktop layout, not a pathological long-content case.

**Why it matters:** A user supervising a Group has to know which Thread they are answering. The most important local control is visually detached from its owning Pane, and the app's navigation chrome sits behind it. This is a composition/ownership problem beyond the previously reported input-routing bugs.

**Cause/evidence in source:** `crates/ferrite/src/cockpit/subagents.rs:1456` sizes Question content against the whole window (`40%`, capped at 320px); `crates/ferrite/src/pane.rs:2822` places the island in a deferred absolute layer above the Composer, escaping Pane clipping. The header/footer/padding add height beyond that content cap.

**Recommendation:** Measure the available height inside the owning Pane, including the title/header and Composer, and constrain the *whole* Question island to that rectangle. Preserve the owning Thread title and make the options area scroll as needed. A deliberate expanded review state may be appropriate when the Pane is too small, but it needs to preserve identity.

**Confidence:** High, native framebuffer plus exact layout path; no mockup or screenshot editing.

### VQ2 — P1 major: The normal L2 grid displays sliced glyphs instead of a readable latest update

**Evidence:** `group-l2-860x500.png` (confirmed in both passes).

**What is actually visible:** All four compact Panes contain fragments of rows: the prompt's raised strip shows only the bottom/top edge of letters, prose is cropped across its baseline, and the bottom-right failure line is sliced horizontally. This affects short ordinary messages after one tool call, not just lengthy transcripts. A status label remains readable below, but the transcript snippets above it look broken and cannot be read as a coherent update.

**Why it matters:** Instruments mode should improve scanning when Panes shrink. Instead, it spends the limited vertical space on several partially visible lines, so the user receives less useful information and a visibly unfinished interface.

**Cause/evidence in source:** `crates/ferrite/src/pane.rs:1654` inserts the conversation tail into the remaining space; `:1686` creates a shrinking flex column with `min_h_0`, bottom justification, and clipping; rows at `:1698` have line clamps but do not establish an intact row height/whole-line selection policy. The header, badges, progress, idle message and Composer compete for the same small vertical budget.

**Recommendation:** Select the newest one or two complete semantic rows that fit, preserving their full line height. Omit older rows as whole rows. Prioritize the active task or latest failure over the historical prompt. The displayed text should never be reduced to a horizontal sliver by flex shrink.

**Confidence:** High, native framebuffer at the project's own everyday four-Pane L2 test size.

### VQ3 — P2 minor: Completed L2 Panes dim the still-active Composer like a disabled control

**Evidence:** Bottom-left Pane in `group-l2-860x500.png`.

**What is actually visible:** The entire completed Pane is faded relative to its working neighbors, including the `Steer this Thread…` Composer and its command hints. The Pane also repeats completion through `done`, `turn complete`, and `idle`, using much of the already constrained height.

**Why it matters:** Completion should help the user find work that is ready for review or follow-up. Applying a disabled-looking treatment to an input that still accepts work makes its interactivity ambiguous, particularly next to normal-brightness inputs.

**Source:** `crates/ferrite/src/pane.rs:1666` assembles header, body and Composer together; `:1668` applies the completed opacity to that entire tree. `crates/ferrite/src/theme.rs:673` sets it to 0.75.

**Recommendation:** Keep interactive Composer controls at their normal contrast. Reduce only secondary historical content, and consolidate the repeated completion labels into one clear state indicator.

**Confidence:** High for current appearance/source; severity is a design judgment, not a functional failure claim.

## Status/Decision quality that works

- L1's failed tool exposes the error detail and red failure wording prominently, without requiring the user to open a successful-tool disclosure first.
- Question radio labels and descriptions have a clear type hierarchy, and the bright `Send answer` action is distinct from Skip. Keep this local hierarchy while fixing its containment.
- Group pane seams and title bands are consistent; the ordinary 1200×800 L1 layout supports reading short progress paragraphs comfortably.
- Completed versus working status remains distinguishable by text, not only color.
- The existing focused-pane outline makes ownership easy to identify until the Question overlay covers the header.

Subagent Main-tab labeling, first-run Project controls, draft empty-state quality, and wide prose measure were handed to their owning audit agents; they are not duplicated here. First-letter permission shortcuts, form focus theft, and request transport defects belong to the earlier functional UX audit and are not repeated as polish findings.
