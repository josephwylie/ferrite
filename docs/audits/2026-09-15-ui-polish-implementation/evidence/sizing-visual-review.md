# Independent sizing review — 18 September 2026

## Final assessment

The reviewed changes resolve the observed everyday sizing and layout problems while retaining Ferrite's established compact density, 286px expanded sidebar, and full-width prose. New Project has an appropriately sized single-directory state and a bounded directory list with pinned actions. Settings controls have consistent height and sensible widths. Short-window navigation keeps full-size targets reachable. Growing drafts and long queues retain their controls. Lists and tables follow their authored structure and alignment.

The compact live status presents one caption with complete elapsed time beside it. The currently displayed streaming thought is not repeated in the transcript tail; prior tool/history content remains above the status. Completed and older thinking behavior is covered by the implementing agent's regression tests. Question cards retain independent scrolling, with the custom-answer field reachable and Skip/Send answer fixed.

Evidence review covers 28 prior native confirmation frames, 20 baseline states at d23c3de, and 20 final scenarios plus five native input-driven interaction frames. The final evidence and reproduction fixture are based uniformly on product commit 051ed18. The affected compact states and input-driven frames were re-inspected after the final changes.

## Verified final behavior

- Settings at 1000x800 and 640x500: provider choices shrink-wrap; menus share width and 32px control height; labels and controls align. The effort menu fits all six choices within the narrow window.
- New Project at 1000x800 and 640x500: single-directory dialog has balanced spacing, readable basename and main-directory identity. Six-directory form scrolls to Integration tests with Add Directory and Create visible throughout.
- Four panes at 1200x800 and 860x500, two at 1000x800, six at 1440x900: controls and metadata fit, retaining existing split geometry and compact/detailed presentation.
- Eight-line draft with eight queued prompts at 860x500: queue, editor, Stop, Send, test metadata, and live progress remain contained. Elapsed time is complete; one current caption is shown. The queue scroll reaches the oldest prompt while editor and actions stay fixed.
- Rail at 640x500 with 14 Threads: scrolling reaches final rows with utility buttons pinned. By Project headers retain their cadence in an overflowing list.
- Standard-size lists: shared marker/text alignment across 9/10/11, wrapped text, continuation, and nested bullets. Large 17px list: 99/100/101 share a measured marker gutter with aligned continuation and nesting.
- Tables at 1000 and 640 width: authored left/right/center alignment remains consistent across semibold headers and body.
- Rust/HTML blocks at 640 width: Copy and Preview targets have consistent padding and remain contained.
- Compact Question with draft: Expand to answer stays accessible. Expanded Question scroll reveals the full custom-answer input, and its action footer remains fixed above the Composer.

## Evidence and limitations

Baseline: /tmp/ferrite-sizing-before-2026-09-18 (20 PNGs at d23c3de).
Final: /tmp/ferrite-sizing-after-2026-09-18 (20 PNGs at 051ed18).
Interactions: /tmp/ferrite-sizing-interaction-2026-09-18 (five PNGs at 051ed18, in rail-scrolled, project-scrolled, effort-menu, queue-scrolled, question-scrolled subdirectories).
Final capture log: /tmp/ferrite-sizing-final-capture.log.

Baseline draft-grow contains an eight-line draft but no accepted queue: the initial synthetic send steered instead. Final draft-grow explicitly enqueues eight accepted prompts and asserts their count. The baseline proves draft/controls clipping, and must not be described as queue evidence. Final lists-large replaces baseline prose-desktop and uses valid five-space continuation indentation for 99/100 numbering.

These are native-renderer captures of synthetic state, with native input dispatched for the interaction frames. They establish geometry and scroll/menu behavior rather than live provider correctness. Root owns separate live CUA checks and full-suite test results.

## Disposable capture fixture

Worktree: /Users/josephwylie/Desktop/Projects/ferrite/.worktrees/polish-sizing-qa-2026-09-18
Only modified file: crates/ferrite/src/visual_reference.rs.
Fixture backup: /tmp/ferrite-sizing-native-fixture-2026-09-18.rs.
Reproduction patch based on 051ed18: /tmp/ferrite-sizing-native-fixture-2026-09-18.patch.
No product edits or repository README were created in the QA worktree.

The harness uses synthetic Session behavior, no external CLI spawner, disabled automatic titles, and unique temporary stores/preferences. It never opens the primary user store. Real-window hold mode uses the title Ferrite Sizing QA.

Build/capture from the disposable worktree, after acquiring root's exclusive shared-target build slot:

```sh
CARGO_TARGET_DIR=/Users/josephwylie/Desktop/Projects/ferrite/target cargo run -p ferrite --locked --features visual-reference -- --visual-reference /tmp/ferrite-sizing-after-2026-09-18
```

After compilation, real native hold mode:

```sh
FERRITE_POLISH_HOLD=settings-narrow /Users/josephwylie/Desktop/Projects/ferrite/target/debug/ferrite --visual-reference /tmp/ferrite-sizing-live
```

Other hold modes include project-many, question-expanded, rail-overflow, draft-grow, lists-large, lists-table-narrow, and code-narrow. FERRITE_POLISH_FILTER chooses a scenario substring; FERRITE_POLISH_SCROLL=x,y,dy and FERRITE_POLISH_CLICK=x,y dispatch native input after the initial layout settles.

Exact interaction reproduction commands, also from the disposable worktree:

```sh
FERRITE_POLISH_FILTER=rail-overflow FERRITE_POLISH_SCROLL=38,275,-2400 /Users/josephwylie/Desktop/Projects/ferrite/target/debug/ferrite --visual-reference /tmp/ferrite-sizing-interaction-2026-09-18/rail-scrolled
FERRITE_POLISH_FILTER=project-many FERRITE_POLISH_SCROLL=300,300,-2400 /Users/josephwylie/Desktop/Projects/ferrite/target/debug/ferrite --visual-reference /tmp/ferrite-sizing-interaction-2026-09-18/project-scrolled
FERRITE_POLISH_FILTER=settings-narrow FERRITE_POLISH_CLICK=369,387 /Users/josephwylie/Desktop/Projects/ferrite/target/debug/ferrite --visual-reference /tmp/ferrite-sizing-interaction-2026-09-18/effort-menu
FERRITE_POLISH_FILTER=draft-grow FERRITE_POLISH_SCROLL=420,200,-1200 /Users/josephwylie/Desktop/Projects/ferrite/target/debug/ferrite --visual-reference /tmp/ferrite-sizing-interaction-2026-09-18/queue-scrolled
FERRITE_POLISH_FILTER=question-expanded FERRITE_POLISH_SCROLL=550,250,-1200 /Users/josephwylie/Desktop/Projects/ferrite/target/debug/ferrite --visual-reference /tmp/ferrite-sizing-interaction-2026-09-18/question-scrolled
```

All 25 final native captures completed successfully. Re-inspection confirmed one current compact caption, complete elapsed time, preserved prior tool history, and intact queue/editor/actions. All five interaction frames passed their geometry/reachability checks. No unresolved sizing or layout issue was found in the reviewed final states. Cargo slot released; no QA build or capture process remains running.
