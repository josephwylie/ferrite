# Evidence and reproduction

All screenshots are fresh captures of Ferrite's production GPUI renderer with synthetic Sessions, taken on 15 September 2026. Product source is `fbc03f3`; the earlier audit commit changes documentation only. PNGs are unmodified at 2× backing scale. Logical window dimensions are recorded in [capture-manifest.json](capture-manifest.json).

## Baseline renderer states

From the audit worktree, run the existing visual-reference harness:

```sh
cargo run -p ferrite --features visual-reference -- --visual-reference /tmp/ferrite-quality-baseline
```

This produces formatting, edges, live, decision, approval, expanded and interrupted states at 720×1400 and 1000×1400 logical pixels. The report retains seven relevant frames. These tall fixtures establish component rendering; their unused transcript height is not evidence of waste in an everyday window. The synthetic commands, failure and completion messages are fixture data, not results of a real user task.

## Everyday Solo and Settings windows

[daily-size-fixture.patch](daily-size-fixture.patch) changes only the capture harness in a disposable source copy: natural prose, Settings open, and 1440×900 / 1000×800 dimensions. It does not change any production component, style or event handler. It is preserved as an **unapplied evidence patch**, not an implementation change.

To reproduce, create a disposable checkout of `fbc03f3`, apply that patch there, and run the same command with a different output directory. The [capture log](daily-size-capture.log) records a successful exit after compiling the disposable copy. A shared Cargo target cache may be used; the captures do not need a real provider or credentials.

## Group, creation and subagent windows

[group-and-creation-fixture.patch](group-and-creation-fixture.patch) replaces only the capture harness in a second disposable copy with six small scenarios. Apply it to a separate clean `fbc03f3` checkout, not on top of the daily-size patch. Then run the same visual-reference command. The [final log](group-and-creation-capture.log) records six successful captures: four-Thread L1 at 1200×800, L2 at 860×500, a two-option Group Question, a registered-project draft, first-run Project editor, and a Thread with three subagents.

The final harness clears synthetic transient notices before capture through the core's existing API. An initial pass captured notices mid-animation; those overlapping toasts were excluded as a timing artifact and are not counted as product defects. Both Group layout findings persist in the clean final frames.

## Native measurements and detector scope

[measurements.json](measurements.json) records source-token sRGB relative-luminance calculations and modeled alpha composites; [the independent review](reviews/measurements-b.md) traces the actual toolkit paths, including its additional opacity. [prose-line-measure.json](prose-line-measure.json) records the ordinary paragraph's wrapped first lines. [settings-corner-samples.json](settings-corner-samples.json) records the rendered modal corner samples. These data supplement visual judgment; they are not a compliance certificate or a perceptual benchmark.

[detector.json](detector.json) retains the required web detector attempt and [detector-scope.json](detector-scope.json) its inventory. It found two warnings on a prototype HTML file and scanned zero Rust files. No production issue is inferred from those warnings. The native source, state calculations and framebuffers are the useful evidence for this application.

## Installed app observations

The lead also inspected the installed Ferrite app through native computer-use tooling: its current two-pane Group and Settings/New Threads, Behaviour and Permissions. [Observation notes](native-observations.md) record this boundary. Settings was closed afterward; no preference, prompt, approval, provider, project or Thread lifecycle was changed. Live screenshots include unrelated user work and are intentionally not copied into this repository.

The installed binary's exact Git SHA is unknown. Its appearance corroborates the source review; the synthetic current-source captures provide reproducible visual evidence.

## Interpretation limits

The report distinguishes framebuffer observations, installed-app observations, source-confirmed behavior and proposed design refinements. It does not claim a measured usability-test outcome. Static images do not establish animation quality, frame pacing, complete keyboard traversal or Windows rendering. Existing functional reproduction evidence remains in the [first audit](../../2026-09-14-full-ux/evidence/reproduction.md).

This is a native GPUI application. A DOM/CSS detector cannot certify its production interface. The independent measurement review records that limitation rather than presenting prototype HTML findings as Ferrite defects.
