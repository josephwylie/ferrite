# Current parity acceptance

Updated 2026-11-17. The target is faithful alignment of the five reported surfaces with T3 commit `4da2bc520c38cd6b36014d2888774168fbf4c77f`. Acceptance remains open. No 1:1 or numerical 90% claim is supported.

All implementation work is in the warmed `ferrite-t3-parity` worktree. The main checkout and prior 2026-11-13 audit are preserved. Nothing was committed, published, merged, or installed as a release.

| Surface | Verified result | Source rules and remaining differences |
| --- | --- | --- |
| Sidebar | Native 256px width, 52px header, 28px header toggle, collapse/reopen and saved resize. Source grouping, type and selected fills are implemented; regression tests passed. | [Contract](sidebar.md). Ferrite branding, Park terminology and existing Factory affordance remain. Native font/raster output is not pixel-identical. |
| PR | Actual requested `josephwylie/ferrite#309` loads beside its conversation, including restored Code. Native Wrap on/off, syntax/gutters, sticky Summary, condensed header and independent tab restoration passed. | [Contract and captures](pr-current.md). Immediate header transition, narrower syntax grammar, local viewed marks, no split/tree/per-commit selection, incomplete Timeline/Guide and provider-backed editing/review controls remain. These are real differences. |
| Context | 280px card persists inline with 12px edge gap, returns after closing a panel, and uses the source’s separate anchored popover and measured density rules. Geometry/density regressions passed. | [Contract](context.md). GPUI has no equivalent backdrop blur/saturation in this renderer; opaque fallback remains. Project scripts, discovered servers, thread-bound automations and direct Git operation projections are not implemented. Remote editor and several usage hints differ. |
| Cite | Native 320px popover, 64px minimum field and 28px buttons. Typing, Enter-save, reopening, cancel preservation and Cmd+Enter save/send to the verified local fixture passed. | [Contract](cite-complete-parity.md), [before Send](evidence/live-browser/ferrite-cite-before-send-build6.png), [sent quote/comment](evidence/live-browser/ferrite-cite-sent-build6.png). Ferrite retains broader cross-message selection semantics. |
| Live annotations | Real CEF renders Select/Region/Draw/Erase and live CSS controls. Final smoke verifies styling, selection, comment/CSS typing, property preview/restoration, drawing, region creation, chosen-target erasure, submission geometry and keyboard isolation. | [Contract and captures](live-annotation-current.md). Native Select, CSS editing, Attach, receipt reopening, Draw, Region, Erase, and direct Send passed after Keychain resolution. The final startup-history correction passed both packaged smoke and normal native startup. Theme follows pinned Code dark rather than a full palette picker. Voice is intentionally absent. |

The old seven-tool retained-image editor is not the live annotation implementation. It remains only for historical screenshot editing. Earlier screenshots of it are not evidence for the live workflow.

The operator resolved Keychain. The normal app now loads the page and native Attach/Send acceptance passed. The [native completion record](evidence/live-browser/native-completion.md) records two additional defects caught through real input: initial script registration and Cmd+A in CSS fields. A strengthened cold-start smoke caught a Back-history regression. Its correction passed the final packaged smoke and normal native startup. Production security settings were not weakened.

## Verification

- Earlier checks for unchanged surfaces passed 176 narrow Rust tests. [Log](evidence/live-browser/final-ui-tests.log). Earlier core and forge checks passed separately.
- The last JavaScript edit passed eight focused tests with `node --test crates/ferrite-pane/src/visual/live_annotation/guest.test.ts`.
- After the final Rust edit, five bootstrap tests passed with `cargo nextest run -p ferrite-pane --features cef --lib -E 'test(bootstrap_document) | test(keyboard_bootstrap)'`. [Log](evidence/live-browser/bootstrap-history-tests.log).
- The final `cargo check --locked -p ferrite --features cef,headless,visual-reference` passed. [Log](evidence/live-browser/bootstrap-history-typecheck.log).
- `scripts/install-app.sh --bundle-only`, with the existing controller stage, produced build eleven. [Build log](evidence/live-browser/bootstrap-history-build.log).
- Build eleven's packaged browser smoke exited 0. It includes cold first-load history, rendered controls, raw-key Cmd+A, CSS preview and restoration, Draw/Region/Erase, capture keyboard isolation, controller checks, and clean shutdown. [Log](evidence/live-browser/bootstrap-history-smoke.log), [structured result](evidence/live-browser/build11-smoke/native-results.json).
- The normal native app then loaded the requested local page, kept it visible on Back, and opened the toolbar and anchored editor. [History capture](evidence/live-browser/ferrite-native-history-final.png), [editor capture](evidence/live-browser/ferrite-native-annotation-final.png).
- The [verification manifest](evidence/live-browser/bootstrap-history-verification.json) records exact source hashes, commands, native results, and cleanup. No production source changed after these checks.

The disk-heavy local-profile test attempt was cancelled before tests ran. Its abandoned outputs, completed test executables, disposable smoke profile, and worktree incremental caches were removed. Free space was 4.7 GiB after final cleanup. Warm dependency outputs, both audit directories, source, the main checkout, and the final verification app remain. The app is still an isolated local bundle; no release was installed.

Prove It Works required normal native input as well as the isolated renderer pass. Fix Root Causes led to the inactive-image focus correction, restored-Code preparation, inherited-wrap correction and capture-time keyboard guard. No measured whole-application performance claim is made.
