# Ferrite UI quality — independent design assessment A

Assessment A complete. Reviewed current-source native framebuffer screenshots supplied by root, without seeing Assessment B, detector output, or its conclusions. No product changes. Files: `/tmp/ferrite-ui-root-fixtures/prose-{desktop,compact}.png`, `settings-desktop.png`, and `/tmp/ferrite-ui-baseline/{formatting-wide,live-narrow,decision-wide}.png`. Backing scale is 2×; typography judgments use source logical sizes, not screenshot pixel dimensions. The baseline tall fixtures are layout evidence, not claims about typical desktop aspect ratios.

## Verdict

Ferrite has a coherent, product-specific visual foundation: a quiet dark instrument panel, modest rounded geometry, status-led color, and a readable distinction between human prompts, prose and execution records. It does not need a new visual identity. It needs more deliberate hierarchy inside the identity it already has. The greatest quality gap is that density is expressed through small fixed type while full-size Solo layouts let prose run across an extremely wide measure. Secondary surfaces also flatten different choices and information levels into similar small gray text. The result feels thoughtfully assembled but not yet consistently finished.

The correct direction is to retain the dark palette, compact control heights and many-pane structure, then make reading scale with the available space, reduce repeated status/chrome emphasis, and give controls clearer grouping. More gradients, accent colors, shadows or general padding would make this worse.

## Priority visual issues

### A1 — P1: Wide Solo prose is physically difficult to track across the screen

**Observed:** In `prose-desktop.png` at 1440×900 logical, the answer's first paragraph stretches nearly the entire wide pane. A normal paragraph becomes a few very long lines. The same prose is substantially easier to track in `prose-compact.png`, although still dense. This is ordinary prose, not an unbroken pathological string.

**Cause:** Answer text stays at 13px (`theme.rs:226`–232) with full-width content (`transcript.rs:314`–348); Markdown is also full width (`rich.rs:244`–250). `pane.rs:4320`–4326 explicitly records an earlier choice to remove the 68-character prose cap. This critique does not treat that product choice as accidental.

**Better treatment:** Keep dense Group panes and full-width tools/diffs. Give Solo a distinct reading treatment: a user-controlled larger prose size and an optional comfortable reading width, rather than silently reinstating a fixed narrow column. If full-width prose is a firm constraint, at least make answer typography scale with Solo width and expose text size. Do not enlarge every metadata label along with it. The existing 13px treatment can remain the dense Group default.

**Why:** The application spends many minutes at a time displaying explanations and results. Readability should improve when the user gives one Thread more space. The current Solo expansion mainly lengthens eye travel.

**Confidence:** Direct fresh screenshot comparison plus source. Recommended width treatment is a product preference to resolve, not a claim that the previously approved full-width choice can be changed without consideration.

### A2 — P2: The New Threads settings page presents a flat wall of choices

**Observed:** `settings-desktop.png` shows Provider, both providers' models and both effort ladders simultaneously. Twenty-plus options use almost identical muted labels; unselected chips share the panel's ground, so they resemble passive text. The content says “New Threads” twice above the fields, while the sidebar already names that page. Visual repetition consumes hierarchy without clarifying it.

**Cause:** `cockpit.rs:2431`–2497 constructs both providers' model/effort fields in one list. `prefs.rs:132`–137 gives the page the same title as its group; `prefs.rs:158`–172 renders every option as wrapped chips. `prefs.rs:209`–221 uses `RAISED` for unselected chips, while `prefs.rs:57` uses `MENU`; both resolve to #282828 in `theme.rs:44`–51.

**Better treatment:** Use a single page heading, then two clearly named provider sections with each model and effort choice aligned as a compact form. Present model as a selected-value dropdown or searchable chooser; preserve the short effort ladder as a segmented group with a visible group boundary. If both provider defaults must remain visible, separate their sections by one intentional gap or subtle rule, rather than hiding one merely because it is not the default provider. Add an explicit selected marker beyond the small fill difference.

**Why:** The operator should scan the current defaults first and reveal alternatives when changing one. Currently the alternatives dominate the page, and the eye must reconstruct which gray items are controls.

**Confidence:** Direct current Settings image and exact color/layout path. This is visual grouping and affordance, not a repeat of provider-default behavior findings.

### A3 — P2: Every prose fragment gets large “answer” spacing, weakening the dense transcript's rhythm

**Observed:** The prose and live screenshots have a conspicuously large break between “Read 1 file” and the next assistant fragment. In a coding transcript that alternates brief commentary and tool calls, each new prose run receives the same roomy branded entry treatment as a substantial answer. The discontinuity is noticeable even when the content consists of one sentence.

**Cause:** Every Markdown answer row gets `ANSWER_PAD_Y = 12px` above and below (`theme.rs:596`–601, `transcript.rs:317`), in addition to the 10px inter-row `BLOCK_GAP` (`theme.rs:615`, `transcript.rs:544`–546). Tool rows use only 1px vertical padding (`theme.rs:608`). A short sentence therefore carries substantial wrapper whitespace relative to its line height.

**Better treatment:** Establish spacing by relationship: 4–6px between commentary and the tool work it introduces, roughly 8–10px between content blocks, and a stronger 14–16px boundary between actual turns. Preserve the Ferrite mark, but reduce the symmetric padding on each prose run. Do not solve this by shrinking the already small prose type.

**Why:** Dense tools and generously padded fragments alternate abruptly. Many short updates can use more space than their information warrants, while true turn boundaries have no correspondingly stronger rhythm.

**Confidence:** Fresh screenshot observation and exact constants. Recommendation is about pacing and grouping, not removal of content or the previous functional history cap.

### A4 — P2: Failure decoration is repeated more strongly than the current state

**Observed:** In the prose screenshot, a single failure appears as a red group summary, a red “1 failed” badge, a red tool verb, a second red “failed” badge, and a red error line. Directly below, “Completed · …” is ordinary muted bookkeeping. The live screenshot repeats the same failure stack while the current “Thinking” status is a quiet footer. The strongest visual block can therefore remain a historical event rather than the pane's current condition.

**Cause:** Group failure count and color are drawn at `pane.rs:4676`–4700; each failed child also draws a separate failure chip at `pane.rs:4480`–4483 and a failure message at `pane.rs:4580`–4598. Current progress is `TEXT_2` with smaller muted metadata at `pane.rs:2472`–2509. Completion is rendered as ordinary Meta text at `pane.rs:4273`–4276.

**Better treatment:** Keep one strong failure summary and the actual error explanation. Remove redundant “failed” decoration from the already exposed failed child, retaining its red icon/verb as needed. Give current pane state a consistent location and typographic weight, so “working,” “needs your input,” “completed,” and “stopped” form one recognizable visual system. Preserve error visibility; reduce repetition rather than washing errors out.

**Why:** High-salience color should help answer “What needs me now?” Multiple badges describing the same event spend that attention budget repeatedly.

**Confidence:** Direct fixtures show the visual repetition. The fixture's failure is synthetic; this is not a claim that a particular real Thread completed incorrectly.

### A5 — P2: Composer and progress controls have an unfinished, text-strip hierarchy

**Observed:** `live-narrow.png` shows “esc to interrupt” on the progress metadata line and “esc interrupt” again immediately below in the Composer controls. The latter abuts “@ files · / commands” without the same middle-dot rhythm used inside those hints. The bottom of the pane reads as several similar gray instruction lines: current state, elapsed/hint, input placeholder, keyboard hints, provider and effort. In the question screenshot, the polished sans-serif Question card sits above a much flatter “Reply to the Decision…” strip, making the combined interaction surface look assembled from two visual idioms.

**Cause:** The progress line appends the escape hint at `pane.rs:2451`–2453; the Composer adds a separate escape label at `pane.rs:2763`–2785 and then a separate hints child at `pane.rs:2790`–2800. They share small muted text but are separated only by a generic flex gap. The primary writing surface and mode controls do not get distinct role treatment.

**Better treatment:** Use one status row with the activity caption and its single interrupt affordance. Keep one clean Composer control row: writing helpers grouped on the left, provider/effort grouped on the right. Introduce a consistent separator or group spacing between categories, and show the relevant Escape hint once. Keep the form's action area visually dominant when a Decision is open; secondary reply affordances should not resemble a competing answer route.

**Why:** The user should distinguish status, an editable prompt, and configuration at a glance. Repeated hints and weak group boundaries make all three read as one band of incidental text.

**Confidence:** Direct fresh live/decision screenshots plus source. This does not rely on text overflowing or being truncated.

## Smaller observations

- H3 through H6 use the exact same scale multiplier (`rich.rs:334`–338). The formatting fixture shows four identical-looking heading levels. This is a minor hierarchy limitation for deeply structured answers; ordinary H2/H3 usage remains distinguishable. A restrained secondary cue such as weight or spacing is preferable to six dramatically different sizes.
- In the formatting fixture, table headers are normal-weight, centered and the same surface as cells (`rich.rs:323`–329). A modest header weight increase and consistent column alignment would make normal comparison tables easier to scan without a louder background.
- The mixed monospace operational chrome and sans-serif prose is defensible for this product, but the pane title and prompt can feel heavier than the answer body. Preserve the code/prose distinction; tune weight before changing font families.
- The very tall baseline examples exaggerate the physical distance between history and the pinned bottom controls. I did not score that empty area as wasted space; pinning the Composer is useful and normal desktop fixtures give the appropriate context.

## Strengths worth preserving

1. The dark surface hierarchy is coherent. Ground, nav, pane, prompt and floating form sit on related grays with restrained radii. There are few unnecessary dividers or decorative cards. The product has a quiet, tool-like character.
2. The answer uses native sans-serif reading text, recognizable Markdown structure, selective syntax color and a small Ferrite mark. Human prompts are visibly distinct without relying on chat bubbles or large avatars. This fits a coding cockpit.
3. The Question card has a clear subject, readable choices, subordinate descriptions and a strong Send answer action. The white primary button is decisive against the dark surface; the small amber accent locates the request without flooding the pane with color.

## Heuristic scores

These are design-assessment scores for the inspected screens and source, not a fresh end-to-end functional test or a replacement for the preceding UX audit. All ten heuristics apply to an operational app. Functional dimensions are conservatively scored from visible affordances and inspected implementation; unseen cases do not receive implied passes.

| # | Heuristic | Score /4 | Design assessment |
|---|---|---:|---|
| 1 | Visibility of system status | 3 | Color and pinned activity communicate state, but repeated historical failure emphasis and duplicated current hints weaken hierarchy. |
| 2 | Match with the real world | 3 | Coding terminology, file/branch signs and prose structure fit the operator; some internal wording leaks into notice lines. |
| 3 | User control and freedom | 3 | Close, Skip, Escape hints and clear pane context are visible; complete undo/recovery behavior was outside this visual pass. |
| 4 | Consistency and standards | 2 | Coherent palette; control affordances and typography differ noticeably between Settings/forms and operational strips. |
| 5 | Error prevention | 3 | Decision choices and the primary submit action are clear; stronger selection markers would improve configuration confidence. |
| 6 | Recognition rather than recall | 2 | Small icon controls and flat choice groups still require hover/interpretation; keyboard hints help. |
| 7 | Flexibility and efficiency | 3 | Dense views and keyboard affordances suit experts; Solo reading does not adapt sufficiently to its available space. |
| 8 | Aesthetic and minimalist design | 3 | Authored and restrained overall, with meaningful rhythm and emphasis issues rather than a need for redesign. |
| 9 | Error recognition and recovery | 2 | Failure is unmistakable but over-repeated; recovery hierarchy is weaker than error emphasis in the inspected fixture. |
| 10 | Help and documentation | 2 | Contextual hints and field descriptions exist; hints are repeated and the visual route to broader help is limited in these screens. |
| | **Total** | **26/40** | **Acceptable foundation; significant hierarchy and readability refinement remains.** |

The score is intentionally not inflated by the tidy palette or lowered for speculative edge cases. No P0 issue was found in this visual pass. One P1 and four P2 recommendations above should drive the next polish pass.

## Cognitive load and emotional journey

The main Thread has few actual decisions while reading; the issue is emphasis, not option count. Settings is the exception: both model rows and both effort rows expose more than four competing choices at once and the page provides little visual separation between them. The Question card is comparatively clear because its text, choice descriptions and primary action form an obvious sequence.

The opening impression is calm and competent. During reading, very wide prose and irregular commentary/tool spacing introduce fatigue. During supervision, a strong red history block can dominate the more subdued current state. At a Decision, the purposeful amber card restores clarity. A consistent “current status → content → action” hierarchy would remove those valleys without adding decoration.

Persona checks: an expert supervising several agents needs restrained, reliable salience more than larger controls everywhere; a reader spending twenty minutes reviewing an answer needs scalable prose; a first-time operator configuring defaults needs current values to be visually distinct from the list of alternatives. These are concrete roles suggested by Ferrite's stated use, not claims from user research.

## Scope boundary

Source reviewed: `theme.rs`, `pane.rs`, `transcript.rs`, `rich.rs`, `nav.rs`, and the specific Settings construction in `prefs.rs`/`cockpit.rs`. Current Group behavior was not personally observed in a fresh screenshot by Assessment A; any Group implications above follow from the same rendered components and must be confirmed by root's Group evidence. No detector was read, no GUI controlled, no files in the repository edited. Prior functional findings about history, diff semantics or missing actions were not recycled as new polish findings.
