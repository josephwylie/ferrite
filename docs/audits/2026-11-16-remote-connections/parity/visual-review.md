# Parent visual review

The parent inspected every original in the initial 69-image review set, using labeled contact sheets for coverage and individual native captures for detailed layout review. That inspection rejected the native-6 new-thread claim: its attachment-ready screenshots show a New project dialog, and the subsequent screenshots show a second turn in the existing thread. The functional summary alone did not catch this. The corrected native-7 run passed after publishing authoritative project identity and tightening the harness. The parent inspected all 49 replacement captures, including the actual remote draft and completed new thread on each route. SSH created thread 2 from thread 1; Remote link created thread 4 from thread 3. The final gallery contains 69 originals: those 49, four actual Codex captures, four compact captures and twelve T3 references.

## Comparison verdict

The Add Environment dialog follows T3's composition: Remote link defaults selected, SSH is the adjacent choice, fields and validation occupy the same hierarchy, and the full-width submit action stays below them. Native Ferrite focus rings, typography, colors, icons and background dimming differ. The Connections settings frame and navigation also remain Ferrite's.

Chat uses Ferrite's normal composer and transcript for remote threads. The native captures show attachments, active tool activity, completion, remote origin, Files, Changes and terminal. Their ownership is established by functional checks, not appearance. Their overall composition resembles T3, but spacing, toolbar controls, transcript presentation and panel arrangements are not identical. T3's terminal reference is below the chat; Ferrite's is a right-side pane. T3's richer diff controls are not replicated.

The 640×420 captures show both connection modes and SSH validation with the submit button visible. Mode descriptions move into tooltips at this height. The enclosing settings content scrolls; its empty-state text extends below the visible viewport. This is not a claim that every settings page fits without scrolling.

| Surface | Inspection result | Difference or qualification |
| --- | --- | --- |
| Connections empty / connected / disconnected / removal | Clear hierarchy and readable state/action placement | Ferrite settings frame, switches and navigation differ from T3 |
| Remote link / SSH dialog | Matching mode/field/footer hierarchy; no clipped action | Native styling and dimmed background differ from T3 blur |
| Host autocomplete / validation / progress / authentication | Popup and errors remain within modal; authentication has explicit cancel/continue | Included test alias deliberately refuses connection after selection |
| Existing chat / image / stream / completion | Remote label, attachment, tool activity and completion visible | Ferrite transcript and composer, not a pixel-identical T3 renderer |
| New chat | Native-7 draft and completed thread inspected on both routes | Distinct IDs, remote provider process, project and cwd asserted; native-6 false positive retained |
| Files read / save | Tree, editor, selection and Saved state visible | Native Ferrite editor arrangement differs |
| Changes / stage / unstage | Hunk and stage control visible; Git index asserted separately | Stage screenshot alone is insufficient; T3 split/file-tree controls differ |
| Terminal | Working directory and marker visible | Right pane vs T3 bottom pane; synthetic shell inherits local shell startup warnings |
| Compact dialogs | Both modes and error footer visible at 640×420 | Mode descriptions are tooltips |
| Real Codex SSH chat | Read token and screenshot description visible | Separate c3743ff8 build; actual model smoke is SSH only |

## Reference provenance

T3 source was inspected at `4da2bc520c38cd6b36014d2888774168fbf4c77f`. The installed reference is T3 Code Nightly `0.1.10-nightly.20261111.4544`, an older build. Its twelve native JPEGs retain the original capture bytes. Chat reference captures use an actual GPT-6.1-Sol response in a synthetic local T3 project; they do not prove T3 remote connectivity.

Ferrite's original PNGs and T3's JPEGs have sidecars recording hash, viewport, dimensions, device pixel ratio, crop and build provenance. Comparisons use equal 1470×923 logical viewports at DPR 2, place original pixel rectangles side by side, and never rescale or retouch the originals. Temporary reduced contact sheets were used only as review aids and are not the comparison evidence.

## Scope that remains different

This PR connects a machine through SSH or one authenticated Remote link. It does not reproduce T3's route aggregation/failover, discovery/relay service, or its separate Permissions/route-management interfaces. It does not replace Ferrite's global chat, Files or Changes renderer with T3's. The evidence supports the implemented remote workflow, not complete visual or functional identity with T3.
