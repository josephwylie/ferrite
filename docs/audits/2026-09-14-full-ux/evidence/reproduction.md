# Ferrite audit reproduction evidence

Original source: `/Users/josephwylie/Desktop/Projects/ferrite/.worktrees/ux-audit-2026-09-14`, HEAD `fbc03f3`, branch `audit/full-ux-2026-09-14`.

Only a throwaway copy was changed: `/var/folders/5y/hnh41b2j29z8kqxy7ftzfdn80000gn/T/ferrite-ux-decisions-kwkw0hkm` (macOS canonical path starts `/private/var/…`). The directory is also recorded in `/tmp/ferrite-ux-decisions-checkout`. Real provider actions were never sent. All Sessions are the existing test fake. The live Ferrite window was not controlled.

## Preserved artifacts

- `/tmp/ferrite-ux-headless-probes.patch`: exact additional probes, applicable to original HEAD in another disposable copy. It only appends tests to `crates/ferrite/src/cockpit/tests/provider_forms.rs` and `crates/ferrite/src/pane.rs`.
- `/tmp/ferrite-ux-settings-probes.log`: complete Settings-only build/test output.
- `/tmp/ferrite-ux-all-probes.log`: complete output for all eight probes, including the four original decision/status tests.
- `/tmp/ferrite-ux-decisions-status.md`: original scope report and findings.

## Exact executed commands

Both commands ran in `/var/folders/5y/hnh41b2j29z8kqxy7ftzfdn80000gn/T/ferrite-ux-decisions-kwkw0hkm`:

```sh
CARGO_TARGET_DIR=/Users/josephwylie/Desktop/Projects/ferrite/target cargo test -p ferrite audit_settings_ -- --nocapture 2>&1 | tee /tmp/ferrite-ux-settings-probes.log
```

```sh
CARGO_TARGET_DIR=/Users/josephwylie/Desktop/Projects/ferrite/target cargo test -p ferrite audit_ -- --nocapture 2>&1 | tee /tmp/ferrite-ux-all-probes.log
```

The pipeline's outer shell exit status is tee's success; Cargo reports test failure in the log. These probes intentionally assert desired UX outcomes and fail against current behavior. This is not a claim that existing repository tests failed. The eight-test run finished: `0 passed; 8 failed; 0 ignored; 0 measured; 352 filtered out` in 0.37s after a 9.62s incremental test build.

Seven probes drive the real GPUI headless window, existing fake Sessions, and production keys. The diff probe calls the actual renderer and captures the exact literal source registered for display/copy; it does not claim a native screenshot comparison.

## Extension results, verbatim key output

Settings Enter:

```text
Enter in Settings sent hidden Composer draft: ["Draft I have not decided to send"]
```

Repro: type draft; Cmd+,; assert Settings open; press Enter. Root cause: Settings card tracks focus but supplies no action scope (`cockpit.rs:2624`), while global `Submit` does not stop for `settings_open` (`cockpit.rs:3128`). P1: a modal action sends a hidden, unreviewed prompt.

Settings Cmd+W:

```text
Cmd+W in Settings parked hidden Thread; settings_open=true, visible_threads=[]
```

Repro: one open Thread; Cmd+,; assert Settings open; Cmd+W. Root cause: `CloseThread` acts on roster focus without a Settings guard (`cockpit.rs:5758`). P1: trying to close Settings stops/parks the underlying Session while the modal stays open. Merge with Enter as one modal action-isolation finding if desired.

201-block horizon:

```text
oldest message missing from entire selectable transcript document: core_blocks=201, core_contains_first=true, ui_contains_first=false, ui_contains_second=true, ui_contains_last=true
```

Repro: stream 201 short text messages separated by ordinary provider `ContentBoundary` events; drain through the real Cockpit pump; compare retained core history with the renderer's full logical text registry. The first item is absent from the entire UI document, not just the onscreen virtual viewport. Root cause: `Level::Transcript.visible_blocks()` returns 200 (`crates/ferrite-core/src/docview.rs:230`); `pane::rendered_window` discards the prefix (`crates/ferrite/src/pane.rs:2380`); only that slice enters `TranscriptView` (`crates/ferrite/src/cockpit.rs:893`). The history remains in core/store. No “load earlier” path was added by the test or found in this rendering path.

Diff indentation:

```text
diff display/copy sources lost indentation: ["if ready:", "return old_value", "return new_value", "make all"]
```

Input included `-    return old_value`, `+    return new_value`, and `+\tmake all`. Capturing the actual renderer's registered literal strings shows all indentation removed. Root cause: `render_diff` calls `.trim_start()` after removing the diff marker (`crates/ferrite/src/pane.rs:5145`). This is intentional prototype-derived rendering per the preceding comment, but makes indentation-sensitive changes impossible to assess faithfully and copied code invalid. No hypothetical long strings involved.

## Original results repeated in same run

```text
valid MCP form entry should submit count=2; got []; composer=2
L2 pending approval: y answered=false; cmd-f fullscreen=false
custom alternative must replace radio pick; got [Answer { picks: [0], other: Some("neither, wait 2 days") }]
a background crash should leave an unread notice
```


## Archive notes from the lead

The relative archived files are `headless-probes.patch`, `headless-probes.log`, `settings-probes.log`, and `composer-probe.rs` / `composer-probe.log`. Temporary paths above document the executed run; they are not permanent requirements. Apply the patch only in a disposable copy of audited commit `fbc03f3`. The preserved patch was never applied to the audit branch.

The lead also compiled and ran the source-module probe using `rustc --edition=2021`, the cached `serde_json` rlib via `--extern`, and `-L dependency=<workspace>/target/debug/deps`. `composer-probe.log` records the actual output. The archived probe uses paths relative to its current location so it reads the unchanged audited source. Initial compilation without the serde_json external dependency failed; compilation with the existing dependency cache succeeded. The Windows output proves duplicate binding entries; actual winning dispatch is source/dependency verified, not a Windows GUI test.

Native UI observations, 14 September 2026: installed Ferrite 0.3.0 development build; current model picker showed names/icons without cross-provider section/warning; Settings About → sandbox search showed an empty right panel until Permissions was clicked; searching the visible option Full access produced no categories; Codex model row had no explicit CLI Default option. Search cleared and Settings closed afterward. No preference, live prompt, real approval, provider choice, or real Thread lifecycle was changed by these checks. The exact binary Git SHA was not available, so these observations corroborate source findings rather than certify binary equivalence.
