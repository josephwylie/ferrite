# Live browser annotation status

Updated 2026-11-17. The requested workflow edits the live browser document before attachment. Implementation is present in the isolated `ferrite-t3-parity` worktree. Native Select, CSS editing, Attach, receipt reopening, Draw, Region, Erase, and direct Send passed after the operator resolved Keychain. The final startup-history correction passed the packaged smoke and normal native startup. See the [native completion record](evidence/live-browser/native-completion.md), which supersedes the historical blocked status below.

## Pinned behavior

T3 source authority is commit `4da2bc520c38cd6b36014d2888774168fbf4c77f` in `/tmp/ferrite-t3-reference`. The imported guest files preserve its MIT notice in `crates/ferrite-pane/src/visual/live_annotation/LICENSE`.

- `apps/desktop/src/preview/PickPreload.ts` owns Select, Region, Draw, Erase, target selection, the compact composer, CSS controls, and teardown. Its marquee limit is 20 elements at line 57. Source context has a five-second deadline at line 58.
- The same file registers keyboard capture at module load at lines 240–250. `preview-pick-preload.ts` imports that module, and `ipc/methods/preview.ts:358` supplies its preload URL. Registration therefore precedes page scripts.
- `PickPreload.ts:1742` permits an absent screenshot. Lines 1748–1756 pass the target union as capture bounds. `Manager.ts:283` documents device-pixel cropping. A mandatory full-viewport PNG does not represent this contract.

Voice is excluded from this task. The source port uses bundled assets without a network runtime dependency.

## Host-owned annotation theme

The initial port kept T3's nullable theme slot but omitted the host's theme-channel assignment. The generated stylesheet still supplied T3's light defaults, so this was a wrong-theme defect rather than evidence of transparent controls. Geometry and event unit tests did not inspect rendered CSS and missed the integration gap.

`AnnotationTheme.ts` now provides every field of `DesktopPreviewAnnotationTheme` as a resolved, immutable host-owned value. Colors follow `packages/shared/src/themePalettes.ts:T3_CODE_DARK_THEME_COLORS`, mapped through `apps/web/src/browser/annotationTheme.ts`; the standard web canvas override comes from `themePalette.ts`, and radius and local font stacks come from `index.css`. The guest installs the complete theme before mounting the closed shadow root. It does not read theme values from the inspected page or fetch fonts.

The real CEF smoke reads computed toolbar colors, borders, group and button dimensions, Select/Draw active states, and CSS-input fill, frame, radius, and fonts. Those assertions explicitly reject the old light defaults and passed in build seven. The pinned generated CSS places `.text-xs` after `.text-sm`, so tool text computes to 12px despite both classes being present; the assertion follows that observed source behavior.

## Live ownership and retained evidence

`visual/live_annotation/guest.ts` runs the editor in a private CEF isolated world and closed shadow root. CSS changes preview on selected DOM elements. Teardown restores original property values and priorities.

`visual/web/cef/annotations.rs` accepts only correlated private DevTools responses. Page console messages and `postMessage` do not authorize submission. Source enrichment resolves retained DOM nodes across execution contexts, so a replacement element with the same selector cannot acquire the original element's source metadata.

`cockpit/browser/annotations.rs` keeps the exact Thread, tab, document generation, native generation, and intervention lease through capture, cleanup, and durable retention. Agent mutation stays paused during that work. Failed restoration retains protection and offers recovery through reload or close. Attach and Send are distinct submissions. Send reaches the composer only after durable attachment and the existing eligibility checks.

`browser_annotations.rs` records typed `LiveFeedback`. Targets include elements, regions, and strokes, with source context and CSS changes. `LiveScreenshot` records either a cropped image with geometry or an explicit unavailable reason. There is no placeholder image. The capture already contains rendered strokes, so retained review does not draw them twice.

The older seven-tool screenshot editor remains for historical capture editing. Its layout captures establish that historical surface only. They do not establish live Select, CSS preview, marquee selection, or live attachment parity.

## Early keyboard capture

The full guest still loads when annotation starts. A small `keyboard-guard.js` now registers three inert capture listeners before the first website navigation. The guard lives in the same private isolated world. Activation binds the exact overlay host and accepts trusted events. Teardown releases the host and callback while retaining only the inert listeners.

There is one intentional difference from the pinned T3 keyboard lifecycle. During pending capture, the guard also suppresses trusted keyboard events retargeted to the page. T3 hides its editor before capture while filtering keys only by overlay targets; the real CEF probe demonstrated that Enter's key-up can consequently reach earlier page listeners before teardown. Ferrite binds a private callback to the existing `pendingCapture` state, keeping this interval protected without timers, polling, or a public bridge. Teardown releases that authority immediately; Escape's key-up after cancellation and subsequent ordinary page input remain available to the page.

CEF preparation waits for a correlated script-registration acknowledgement, with a five-second bound. The queued website navigation starts once on acknowledgement, error, or timeout. Failure preserves ordinary browsing and reports annotation as unavailable. The private empty document cannot replace the requested URL, title, or history state. Agent targets become available after preparation. There is no periodic JavaScript polling during bootstrap.

## Verification and remaining acceptance

The worker ran `node --test crates/ferrite-pane/src/visual/live_annotation/guest.test.ts` after the pending-capture keyboard correction and final JavaScript edit. All eight tests passed, including coverage that every theme token consumed by the generated CSS has a resolved host-owned value and that capture remains isolated after focus moves to the page, until teardown restores ordinary input. Generated `guest.js` is refreshed. Python smoke sources parse, and `git diff --check` passes.

The parent owns Rust checks and the native run. The new Rust filters are `keyboard_bootstrap` and `bootstrap_document_cannot_replace_requested_website_metadata`. The parent reported the relevant Rust tests and typecheck green before the theme correction. The pending-capture guard correction still needs the parent's final build/check and runtime acceptance.

The existing packaged smoke driver includes `scripts/probe_annotation_keyboard.py`. It loads the actual bundled guest after page capture listeners already exist and sends trusted CEF keyboard events. Build seven passed comment/CSS text entry, live CSS preview and restoration, and actual Draw/Region/Erase interactions and submitted target geometry. The remaining keyboard assertion exposed Enter key-up reaching the page during capture: `/tmp/ferrite-parity-build7-keyup.log` records `['Enter']` after attach and `['Enter', 'Escape', 'Enter']` after the later cancel and draw-attach sessions. The private pending-capture correction above addresses that observed failure; its rebuilt runtime gate remains pending. These checks do not replace native cockpit retention and Send acceptance.

Build five confirmed that browser address typing works after correcting repeated image restoration in an inactive Thread. The website itself remained grey. A native process sample showed Chromium waiting in `SecItemCopyMatching` and `SecKeychainItemCopyContent`. The existing isolated mock-keychain smoke loaded both HTTP pages, painted them, and passed its controller and select-popup checks. Its shutdown watchdog prevented a successful process exit, so the full smoke is not green.

The diagnostic command used the packaged `ferrite --visual-smoke` with `FERRITE_BROWSER_SMOKE=1`, `FERRITE_BROWSER_SMOKE_MOCK_KEYCHAIN=1`, the existing `scripts/probe-embedded-browser.py` driver, and a disposable browser profile. The retained runtime log is `/tmp/ferrite-browser-build5-mock-keychain-smoke-corrected.log`. The keychain sample is `/tmp/ferrite-browser-build5-main.sample`. An earlier invocation omitted the required driver and supplies no loader evidence.

Computer Use could not access SecurityAgent. The operator was asked to handle the prompt. No keychain or security setting was changed. The corrected keyboard smoke and native cockpit live-tool interactions, CSS preview and restoration, cropped or unavailable-image attachment, and authorized Send still require acceptance. Historical screenshots remain preserved.

## Final build-eight result

The packaged CEF smoke exited 0. Its [runtime log](evidence/live-browser/final-browser-smoke.log) and [structured result](evidence/live-browser/build8-smoke/native-results.json) record rendered Code dark styling, CSS text entry, live property preview and restoration, Enter attachment intent, Escape cancellation, repeated sessions, drawing two strokes, creating a region, erasing only the selected stroke, and submitting the remaining geometry. The final page-key trace is empty after Attach and contains only Escape after cancellation; ordinary page keys resume after teardown. The final runtime shuts down cleanly. This supersedes the earlier failed smoke gates above.

Captures show the [toolbar](evidence/live-browser/build8-smoke/annotation-toolbar.png), [expanded CSS controls](evidence/live-browser/build8-smoke/annotation-expanded-css.png), and [retained stroke and region after Erase](evidence/live-browser/build8-smoke/annotation-draw-region-erase.png). These use the real packaged CEF engine and generated guest asset used by the bundle. The browser fixture is 400×600 logical pixels. Its page background is temporarily white for visibility and restored before the independent Rust pixel check. The earlier [actual T3 expanded editor](evidence/live-browser/t3-live-expanded-reference.png) uses a different viewport and selected element; it supports control comparison, not a pixel-difference score.

The normal native GUI remains blocked in Keychain. Build eight was relaunched without the smoke flag; its [window](evidence/live-browser/ferrite-browser-keychain-block-build8.png) remains blank, and [process sample](evidence/live-browser/final-native-keychain.sample) shows the CEF worker waiting in SecItemCopyMatching, SecKeychainItemCopyContent and SecurityServer decrypt. Full Cockpit Attach, durable receipt reopening, and Send acceptance remain open. The verification app and its isolated fixture server are left running for the operator to resolve the prompt. No release was installed.


## Final native and startup result

The operator resolved Keychain. Native build ten passed live CSS editing, Attach, retained reopening, Draw, Region, Erase, and direct Cmd+Enter Send to the verified fixture provider. Native Cmd+A now replaces the selected CSS value. The earlier CDP editing command was removed from the probe because it masked the real native defect.

Build eleven passed five bootstrap tests, the app typecheck, and the complete packaged browser smoke after the startup-history correction. The normal app then loaded the requested page, kept it visible on Back, and opened the anchored annotation editor. See the [native completion record](evidence/live-browser/native-completion.md) and [final verification manifest](evidence/live-browser/bootstrap-history-verification.json). Earlier Keychain-blocked statements above are historical.
