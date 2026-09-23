# Ferrite UI quality extension — Composer and controls

Audited `fbc03f3`, branch `audit/full-ux-2026-09-14`. No product changes. Applied the existing Soft system and the better-ui principles; this is a refinement audit, not a replacement visual direction.

Reviewed the freshly generated screenshots `/tmp/ferrite-ui-root-fixtures/prose-desktop.png`, `/tmp/ferrite-ui-baseline/live-narrow.png`, and `/tmp/ferrite-ui-baseline/approval-narrow.png`. Root confirmed the baseline captures are current to this branch. Source review covered Composer, picker renderers, pointer states, attachments, image preview, and theme tokens. I did not control the GUI. Where a state was not captured, it is explicitly source-confirmed rather than claimed as a screenshot observation.

## Findings

### UIC-1 — P1: The Composer hides its primary action while prominently exposing secondary controls

**What is visible now:** At normal desktop size, the bottom surface has a muted text row, `↑ history · @ files · / commands`, and recognizable model/effort dropdowns. There is no Send button, send icon, or Enter instruction. Running state adds `esc interrupt` as more plain gray text. A user can discover how to change a model by looking at the interface, but cannot discover how to submit the text—or how to insert a newline—from the interface itself. Clicking the visible interruption words does nothing.

**Evidence:** The complete Composer layout in `crates/ferrite/src/pane.rs:2669–2841` builds the editable row followed by metadata and pickers; no submit control exists. `:2866–2872` exhaustively defines hints without Enter/Shift+Enter. `:2772–2777` creates interrupt as a plain Div, with no interaction. The keys exist at `crates/ferrite/src/keymap.rs:141`, `:152`, `:155`. All three inspected screenshots show this composition; the desktop capture is especially clear because lack of space is not the cause.

**Specific refinement:** Preserve the small, neutral, keyboard-first control language. Add a compact, unmistakable Send action at the prompt's trailing edge and a quiet `Enter to send · Shift+Enter for newline` hint when composing. Make interruption a real click target using the same action slot/state. Keep the model and effort controls secondary; a large colored call to action is unnecessary.

**Intent versus quality:** A keyboard-first application need not make the keyboard the only discoverable path. The proposed control makes the existing action visible; it does not require changing submission semantics.

**Confidence:** Screenshot-observed + source-confirmed. **Old audit overlap:** New affordance/visual-hierarchy finding. Earlier C8 concerned what interrupt does to queued prompts, which is a separate behavior and should not be counted again here.

### UIC-2 — P2: Suggested prompt text and real editable text have the same visual state

**Normal scenario:** A Thread finishes and offers a predicted follow-up. Before Tab, the suggestion is an overlay outside the actual input; after Tab, it is real text ready to edit/send. Ferrite gives those states the same face, size and ink. The only incidental visual changes are the caret position and disappearance of a separate `⇥ accept` hint. The idle placeholder also has the same ink as the user's prompt.

**Visual consequence:** The Composer does not visibly distinguish “Ferrite is suggesting this” from “this is now my draft.” Accepting a suggestion lacks clear static confirmation, and a prompt-like ghost can be mistaken for text already entered. This is a state-clarity defect, not a claim that suggestions send automatically.

**Evidence:** `crates/ferrite/src/pane.rs:2644–2646` establishes `FS_MD` and `TEXT_2` for typed text. `:2690–2700` overlays the placeholder with the same `TEXT_2`, inherited typography and no distinct styling. Actual input shaping consumes that inherited color at `crates/ferrite/src/composer.rs:941–956`. `crates/ferrite/src/cockpit.rs:3324–3327` copies the accepted text into Composer. `pane.rs:2689` merely adds the caret-width inset while focused. The current plain placeholder appearance is visible in all inspected captures; before/after suggestion acceptance was not captured.

**Specific refinement:** Give ghost suggestions a clearly quieter but readable visual treatment, and promote entered/accepted text to the established primary text token. Retain the explicit Tab acceptance hint. Keep the change immediate and static; no entrance animation is needed for a frequent operation.

**Confidence:** High source confidence; initial placeholder screenshot-observed, suggestion transition not exercised. **Old audit overlap:** New visual-state finding. Earlier audit counted explicit acceptance as a positive and did not flag its styling.

### UIC-3 — P2: Running Threads keep model/effort controls visually enabled even though choices are unavailable

**Normal scenario:** While an agent is visibly working, open either model or effort. The control looks normal, has hover treatment, and shows live selectable choices. Choosing one then produces an “unchanged” failure because the running state is already known to prohibit that change.

**Visual consequence:** The control does not communicate its disabled state before interaction. A routine dropdown looks available, closes like a successful pick, and leaves the user to discover a failure elsewhere in the transcript. This makes the finish feel incomplete even though the core safely preserves the existing settings.

**Evidence:** `crates/ferrite/src/cockpit.rs:4388–4410` opens the model picker without a busy-state presentation; `:4435` sets `fixed = false` for all model rows. Effort rows at `:8031`, `:8043` are likewise not inert. The actual refusal is at `crates/ferrite-core/src/cockpit.rs:2787–2794` and `:2854–2861`. Failure notices are appended by UI `cockpit.rs:4519–4523` and `:8063–8067`. The running screenshot shows the model/effort controls retain their ordinary appearance, although the open menu was not captured.

**Specific refinement:** Reflect availability at the control/menu seam: disabled choices with a concise “Available when this turn finishes” explanation, retaining the current value and predictable focus. Preserve the current core semantics; queuing setting changes would be a separate product decision.

**Confidence:** High source confidence; active-looking closed controls screenshot-observed. **Old audit overlap:** New visual availability/feedback finding. The earlier report described core busy refusals as a safety positive, not a defect.

### UIC-4 — P2: Image preview has no detail-inspection affordance beyond fitting the whole image

**Normal scenario:** Attach a screenshot of code or an error dialog, then open it to verify the relevant text before submitting. In a Group, its preview stays inside that Pane and shrinks the whole screenshot to fit. The sole visible action is Close. There is no 100%, zoom, pan, or Open Original control.

**Visual consequence:** The “Preview image”/maximize affordance promises a closer look, but ordinary screenshot details become too small to inspect. For example, a 1,920px image displayed within a 600px Pane is constrained to at most 540px before padding: text is roughly 28% of its original pixel size. This is a normal screenshot-review case, not hypothetical long-label overflow.

**Evidence:** `crates/ferrite/src/attachments.rs:138–145` supplies a maximize icon and “Preview image” tooltip. `crates/ferrite/src/attachment_preview.rs:113–117` fixes the preview at 90% Pane width/85% height with a 48rem cap; `:138–147` offers only Close; `:151–156` uses a permanently contained image. Preview state at `:20–23` has no scale/offset. The dialog is explicitly bounded to its owning Pane at `:160–165`.

**Specific refinement:** Keep the Pane-owned preview and aspect-ratio-preserving fit default. Add compact Fit/100% and Open Original controls in the existing header; pan only when zoomed. The basic Open Original escape hatch would provide an immediate readable inspection path without expanding the dialog architecture.

**Confidence:** Source-confirmed; no screenshot preview captured in this pass. The numerical example is an explicit illustration of the current size rule, not a measured screenshot. **Old audit overlap:** New preview-finish finding, not included in prior composer findings. If the parent's prior full audit already identified it, merge it rather than count twice.

### UIC-5 — P3: Moving focus between Panes shifts the prompt's text origin

**Normal scenario:** In a Group, click between two Panes containing drafts. Each unfocused Composer reserves a `›` glyph and an 8px gap before the text. Focusing removes both, moving its text left; unfocusing moves it right again. This happens with a short prompt such as “Review the change,” not only near a wrapping threshold.

**Visual consequence:** The edit surface moves precisely when the user lands on it. Repeated Pane switching creates a small lateral twitch instead of a stable text gutter. It also weakens alignment between adjacent Pane prompts.

**Evidence:** `crates/ferrite/src/pane.rs:2708–2725` conditionally inserts the glyph only when `!focused`; the gap is `EVENT_GAP` (8px at `theme.rs:603`). The focused caret is drawn inside the text rather than occupying that gutter. Source implies glyph width plus 8px of movement, about 15px with the documented mono metrics, but this exact distance was not measured live.

**Specific refinement:** Reserve the leading gutter in both states; hide or change its mark without removing its layout box. Keep caret and text origin fixed through focus changes.

**Confidence:** High source confidence; no paired focused/unfocused capture in this pass. **Old audit overlap:** New spatial polish finding, unrelated to older draft-preservation or editing defects.

## Additional inspected areas and why they are not extra findings

- **Menus:** Slash/@ menus have a coherent 30px row rhythm, name/detail hierarchy, match highlights, a selected Enter indicator, and a keyboard-help footer. The floating surface uses the incumbent radius and layered shadows. I did not count a different width for model choice menus as a defect by itself. Missing handover descriptions and stale catalog callbacks already belong to the earlier functional audit and are intentionally not repeated.
- **Hover/pressed states:** Pointer roles consistently distinguish row, raised, carried and text surfaces (`pointer.rs:26–58`). Neutral fills preserve the no-accent system; there is no reason to add colorful hover effects. Small control density is intentional for the cockpit and was not penalized as if this were a touch UI.
- **Attachments:** Pending and delivered files share the kit card family, thumbnail preview and removal tooltips. The island's neutral surround and stock card border are deliberate existing decisions. I did not claim its larger corner radius or system face is automatically wrong. A repeated “Attached” subtitle could carry more useful file information, but without an observed ambiguous pair it is a lower-value optional refinement rather than another scored finding.
- **Preview mechanics:** Closing returns focus, repeated opens replace one preview rather than stacking dialogs, and its image keeps its aspect ratio. Those are good foundations; UIC-4 is specifically the missing detail-view affordance.
- **Motion:** Attachment entrance is a short 140ms ease-out and explicitly uses the kit's reduced-motion-aware mechanism. No unnecessary animation finding was invented. Motion was reviewed in source, not at 10% playback.
- **Contrast:** Existing text tokens were not called illegible merely for being gray. The actionable issue here is similarity between distinct prompt states, not a blanket instruction to brighten the whole UI.
- **Icons:** Main controls retain the established SVG icon family and state colors. Text `⌵` in draft setup and an SVG chevron in model choices are a minor consistency opportunity, but not promoted to a finding without the requested draft capture.

## Outcome and limits

**Five findings: 1 P1, 3 P2, 1 P3.** The primary action affordance is the first fix; suggestion/availability states and readable preview follow; stable focus geometry is the finishing pass. Preserve the current neutral palette, compact density and controls rather than redesigning the Composer.

All findings are bounded to current source and ordinary operating scenarios. Screenshots establish current overall appearance; interaction states lacking captures are labeled as such. No GUI manipulation, source edits, tests, provider requests, or live Session mutations were made by this subagent. Final `git status --short` was clean. The report is stored only in `/tmp`.
