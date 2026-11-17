# Scroll runtime correction

Runtime-only follow-up to [scroll-quality.md](scroll-quality.md). Source slice: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`; existing `target/debug/ferrite` SHA-256: `8f47873505f48c90a267ef5b02913a345b72e390d17b78cd3a72c552497a2b98`. The app ran in headless GPUI against a new temporary Store and a deterministic fake Claude protocol peer. User prompt, Find, disclosure, End, PageUp, and PageDown were dispatched through the public `window.*` commands. No real provider/model was used.

## Paging result is inconclusive

The first review of screenshots `09-pageup.png`, `10-pagedown.png`, `15-pageup-complete.png`, and `16-pagedown-complete.png` incorrectly treated the missing transcript as a paging defect. Those captures also lose unchanged app chrome: the sidebar, thread title, and composer disappear. This app-wide partial frame makes them inconclusive evidence of transcript state. `End` captures show content again, but that does not establish what happened to the underlying scroll offset.

The command log records `PageUp` and `PageDown` as handled, and successful screenshot responses, but no scroll offset, transcript item bounds, or drawn-selector state were captured after either key. `headless.log` has the initial empty-project setup panic and then a clean attach of the restarted window; it contains no paging error. `server.log` records provider discovery, normal thread parking, and normal shutdown. The peer protocol confirms the controlled stream sequence, but carries no viewport state.

The headless runner's `shot` waits six simulated frames, calls `window.draw` twice, then renders the retained scene (`crates/ferrite-cockpit/src/headless.rs:248-270,387-405`; `vendor/gpui-pre/src/window.rs:3156-3210,2745-2748`). Normal draws layout and paint the root tree, while cached subtree reuse remains in the renderer (`window.rs:3407` and cached paint reuse paths). The capture code does not provide an independent geometry/state readout or force the uncached `force_render` path. This leaves open a render/cache capture artifact; it does not prove one.

**Paging verdict: inconclusive.** Do not count the screenshots as a reproduced product bug or close the paging quality check. A follow-up needs independent runtime evidence such as transcript selector bounds/content after each key, or native-window observation. I did not relaunch, build, run tests, or inspect the native window for this correction. Wheel, scrollbar drag, resize, and native macOS key routing remain unverified.

## Other requested paths

- **Find while the tool call is folded:** `⌘F` for `amber compass` reported `no matches` while `Read scroll-fixture.txt` was folded (`05-folded-find.png`). After opening the call, Find for `Folded passage 090` returned `1/1`, even though the output still displayed only its first few lines plus `… +167 lines` (`07-expanded-find-last.png`). This confirms folded-call exclusion and search after disclosure in this controlled case.
- **Find selection visibility:** after the stream completed, Find for `END_SENTINEL` returned `1/1` and the highlighted occurrence was visible in the answer (`18-find-sentinel.png`). This is a positive branch for the latest answer; it does not certify all long-row or hidden-output positions.
- **Incremental append:** the fake peer paused after `APPEND_BASE`, then emitted eight separate stream deltas and a final assistant frame. The final screenshot shows `APPEND_01` through `APPEND_08` and `END_SENTINEL` in order (`12-after-append.png`); protocol timestamps and frames are in `provider-emitted.jsonl`. Append was released after the paging captures with app-wide chrome missing, so this does not establish preservation of a manually chosen reading position during live append.

## Reproduction and limits

1. Start the isolated runtime with `evidence/scroll-correction/drive.py prepare`, then `launch`; register the scratch project with `project open --cwd <scratch-work>` if the startup dialog appears.
2. Start a Claude thread in that scratch directory. Click `composer-row`, type `Read the controlled scroll fixture and summarize it in detail.`, press Enter. The fake peer returns a 90-passage Read result and waits at stage 1.
3. Use `⌘F` and type `amber compass`; capture the `no matches` result. Close Find and click `tool-call`; search `Folded passage 090` to see `1/1`.
4. Press Escape, then End, PageUp, PageDown, and End, taking a shot after each key. The recorded PageUp/PageDown shots are app-wide partial frames and do not establish transcript state; End's shot includes the response again.
5. Write `1` to `evidence/scroll-correction/stage`. The peer appends eight deltas, then finishes. Search `END_SENTINEL` to verify a visible active match.

Evidence includes the exact command log, provider host/emitted protocol logs, peer/driver, isolated Store and process metadata, screenshots, and `cases.json` under `evidence/scroll-correction/`. The headless surface is fixed-size and has no wheel, resize, or thumb-drag command; motion quality and native platform dispatch remain unverified. The initial empty Store opened a New Project dialog; dismissing it caused a headless panic. Registering the scratch project first and restarting the isolated headless process avoided that setup path. This setup panic is recorded in `headless.log` and is not counted as a scroll finding.

No source implementation, build, or test was changed or run.
