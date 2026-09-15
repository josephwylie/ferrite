# Ferrite UI quality — independent Assessment B

Audit only. Product source `fbc03f3`; audit branch `audit/full-ux-2026-09-14`, HEAD `6ae1e1b`. No repository files changed by this assessment. Assessment B was completed without reading Assessment A's report or scores. Evidence is native Rust/GPUI source, the installed `gpui-component 0.6.0` implementation, calculated state contrasts, and current native framebuffer fixtures. No product score is derived from the web detector.

## Evidence and limits

- Detector stdout: `/tmp/ferrite-ui-detector.json`; stderr: `/tmp/ferrite-ui-detector.stderr`; scanner inventory: `/tmp/ferrite-ui-detector-scope.json`.
- Native contrast/state data: `/tmp/ferrite-ui-measurements.json`. Standard sRGB relative luminance; opacity states are modeled composites over the named background, not claims of pixel-exact GPU blending. Ratios are descriptive measurements, not a complete accessibility certification. Anti-aliased glyph edges, display brightness and scaling affect perceived legibility.
- Screenshot inspection: `formatting-wide`, `expanded-wide`, and `decision-narrow` in `/tmp/ferrite-ui-baseline/`; all 14 baseline framebuffers inventoried for dimensions/color counts. They are 720/1000 × 1400 logical pixels, with 2× backing.
- Daily window check: `/tmp/ferrite-ui-root-fixtures/{settings,prose}-{desktop,compact}.png`, 1440×900 and 1000×800 logical pixels, 2× backing. Corner samples: `/tmp/ferrite-ui-settings-corners.json`; manually transcribed line counts: `/tmp/ferrite-ui-prose-measure.json`.
- No GUI interaction was performed by this agent. Keyboard/hover/completed-state findings below are explicit source-path evidence unless marked framebuffer-confirmed. No Windows runtime inspection, screen-reader audit, motion capture, performance benchmark or font-scaling validation is claimed. Existing functional UX findings are in the separate UX audit.

## Required Impeccable detector attempt

Ran exactly `node ~/.agents/skills/impeccable/scripts/detect.mjs --json crates/ferrite/src`. Exit succeeded, stderr empty. It returned **2 warnings, 0 errors, both on one HTML prototype**:

| Warning | Scanned file | Reported evidence |
|---|---|---|
| `flat-type-hierarchy` | `crates/ferrite/src/nav-project-sections.prototype.html:38` | 8.5, 9, 10, 10.5, 11, 12px; overall ratio 1.4:1 |
| `monotonous-spacing` | same prototype, detector line 0 | ~8px appears 19/30 times (63%) |

The actual scanner's candidate list contains **2 HTML prototypes and 0 Rust files**: `nav-project-sections.prototype.html` and `nav-soft-surfaces.prototype.html`. `SCANNABLE_EXTENSIONS` in `/Users/josephwylie/.agents/skills/impeccable/scripts/detector/node/file-system.mjs:26` supports HTML/CSS/JS/TS and web component formats; it excludes `.rs`. These warnings are not defects demonstrated in Ferrite's production native UI. The run gives **no native cleanliness verdict**. Native Markdown actually has separate 18, 15.6 and 13.8px heading sizes (`crates/ferrite/src/rich.rs:330`), so transferring the prototype's flat-type warning would be misleading.

## Findings

### B1 — P1: tool disclosure keyboard focus is invisible

**Scenario:** In a transcript with several tool/reasoning/turn-change disclosures, press Tab to move among them, then Enter to open one.

**Current behavior:** Tab changes the targeted disclosure and focuses its handle (`crates/ferrite/src/cockpit.rs:3420`), but the target adds only `track_focus` and a key context. There is no targeted background, ring, foreground change or `focus_visible` paint in `crates/ferrite/src/pane.rs:4929`, specifically `:4973`. The caller at `crates/ferrite/src/transcript.rs:484` adds the pointer handler, not a target indicator. The comment at `pane.rs:4970` promises a visible ground which the implementation does not provide.

**Impact:** The operator cannot identify which disclosure Enter will activate. This is a failure of a current keyboard workflow, including ordinary short rows.

**Recommendation:** Give the keyboard-targeted header a visible focus treatment, distinct from expanded/collapsed and pointer hover. Preserve the existing full-header click area.

**Confidence:** High, complete source path. A before/after Tab framebuffer would be useful confirmation; this agent did not drive the app.

### B2 — P2: toolkit keyboard focus is substantially dimmer than the Ferrite token suggests

**Scenario:** Tab through Settings choices or Project-editor actions.

**Current behavior:** `crates/ferrite/src/components.rs:13` makes common controls ghost buttons with `border_0`. Settings chips opt into Tab at `crates/ferrite/src/prefs.rs:205`; Project actions at `crates/ferrite/src/project_editor.rs:207` and `:254`. Ferrite sets `theme.ring = #5e5e5e` in `crates/ferrite/src/theme.rs:780`. The installed toolkit paints its outer focus ring at **3px and 50% opacity** (`/Users/josephwylie/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-component-0.6.0/src/styled.rs:11`, `:240`, `:249`). Its full-color border has no width on these borderless controls. Over the Settings `#282828` ground, the outer ring models to `#434343`, only **1.490:1**. Reading only the raw ring token would instead imply 2.274:1.

**Impact:** Focus is difficult to follow among dense small choices even though the controls are keyboard-operable. Focus conventions also differ: custom Question actions use an explicit amber border (`crates/ferrite/src/cockpit/subagents.rs:1887`, `:1905`), while Pane focus is a full-opacity neutral 1px border.

**Recommendation:** Specify and verify one native keyboard-focus recipe at its final composed color and geometry. Retain distinct hover/selected states rather than using those to imply focus.

**Confidence:** High for implementation and computed contrast; no interaction/framebuffer confirmation of focused Settings chips.

### B3 — P2: focusing a Decision or blocked Pane paints over its alert edge

**Scenario:** Focus a Pane currently waiting for a Decision, or a blocked Pane in a Group.

**Current behavior:** `crates/ferrite/src/pane.rs:858` chooses amber/red for the Pane's 1px border. `pane_shell` paints that border at `:1058`. `focus_wrapper` paints another 1px border over exactly the same rectangle/radius, after the shell, at `:1073`–`:1089`; width is `crates/ferrite/src/theme.rs:500`. Consequently the focus edge covers the state edge. The older explanatory comment at `pane.rs:851` says these are independent channels, but the implemented geometry no longer separates them.

**Impact:** A Pane changes from an alert-colored outline to neutral grey when selected, weakening status scanning precisely while the operator handles the problem. The status dot and Decision surface remain visible; the finding is the lost perimeter signal, not total loss of state.

**Recommendation:** Preserve the alert perimeter and represent focus with separate geometry or an explicit combined-state treatment.

**Confidence:** High, code and current `decision-narrow.png` framebuffer agree (grey focused perimeter, amber Decision content).

### B4 — P2: completed compact Panes dim the results and the still-usable Composer

**Scenario:** A Group Thread finishes; the operator reads its result in L2 or locates its title in L3 and continues it.

**Current behavior:** `crates/ferrite/src/pane.rs:1666` appends the L2 Composer before applying `DONE_CELL_OPACITY = 0.75` to the entire content. L3 applies `DONE_WALL_OPACITY = 0.6` to the complete cell at `pane.rs:1410`. Both values are in `crates/ferrite/src/theme.rs:673`. An unfocused L3 title is 11px `TEXT_2` (`pane.rs:1341`–`:1343`), so its modeled contrast falls from **7.539:1 to 3.516:1** on `PANE`. L2 muted metadata falls from **5.985:1 to 3.919:1**. L2 Composer content is faded by the same parent opacity despite accepting input.

**Impact:** Completed work becomes harder to find/read and its editable prompt looks inactive. Completion should reduce noise without fading the content the operator has just been waiting to read.

**Recommendation:** De-emphasize the running indicator/status decoration rather than the entire content and input region; keep completion text/title/input at their normal reading contrast.

**Confidence:** High source evidence; opacity numbers are modeled, no completed Group screenshot inspected by this agent.

### B5 — P2: useful diff annotations use the dim separator color

**Scenario:** Review an expanded write/edit result and identify line numbers or the count of omitted lines.

**Current behavior:** The production diff renderer assigns `SEP #6e6e6e` to line numbers at `crates/ferrite/src/pane.rs:5162` and the “more lines” count at `:5199`. It uses 12px text with a 20px rounded row height at `:5095` and `:5100`. `SEP` has **3.516:1** on `PANE`, and only **2.902:1 / 2.961:1** over the added/removed washes. The actual changed-code inks are much stronger: **8.825:1 / 7.527:1**. `render_tool` calls this renderer in the expanded/ungrouped production path at `pane.rs:4606`.

**Impact:** Operators can read the changed code but need more effort to locate its line or notice how much of the change is omitted. These are meaningful review annotations, not decorative separators.

**Recommendation:** Use a text/metadata token for line numbers and omission counts; reserve `SEP` for nonessential seams/glyph decoration.

**Confidence:** High code and calculated-state evidence; no claim that all diff text has poor contrast.

### B6 — P2: Project “Create”/“Done” lacks the visual priority its role needs

**Scenario:** Add a Project, pick a directory, then look for the action that completes the form.

**Current behavior:** `crates/ferrite/src/project_editor.rs:201` returns the same transparent ghost button as secondary actions, with a slightly brighter label and 28px height. It never adds `.primary()` or a filled background. The caller at `crates/ferrite/src/cockpit.rs:2977` supplies Create/Done and a handler without styling. Secondary action construction is at `project_editor.rs:252`; common ghost style at `components.rs:13`. Toolkit ghost resting backgrounds are transparent in `gpui-component-0.6.0/src/button/button.rs:897` (installed dependency prefix as in B2).

**Impact:** The confirmation control reads as another small text action in the footer. This differs from the clear primary/secondary hierarchy of the Question surface. The source comment calling it the “one filled control” is not reflected by the implementation.

**Recommendation:** Give the final form action a consistent primary treatment and a visually distinct disabled state, preserving the neutral visual language.

**Confidence:** High source evidence; Project screenshot not inspected in B.

### B7 — P2: Settings has one square corner on an otherwise rounded floating surface

**Scenario:** Open Settings at ordinary desktop or compact desktop window size.

**Current behavior:** The bottom-left corner is square, while the other corners remain rounded. Confirmed in both daily-size Settings captures. The card specifies a 10px radius and `overflow_hidden` at `crates/ferrite/src/prefs.rs:44`–`:56`; its inner sidebar separately paints `MENU` at `prefs.rs:123`–`:128`. The toolkit's sidebar/panel path (`gpui-component-0.6.0/src/setting/settings.rs:176`, `:390`) reaches the lower-left edge with its own background. The corner sample at framebuffer `(620,1579)` in desktop and `(180,1479)` in compact is exactly **#282828** inside the nominal rounded corner; corresponding bottom-right extreme corners show the backdrop.

**Impact:** An always-visible modal shell looks incorrectly clipped. This is a concrete current polish defect independent of content length.

**Recommendation:** Make the composed sidebar/card clipping respect the shared corner shape; verify all four corners on the native renderer.

**Confidence:** High for rendered defect and pixels; specific clipping mechanism is a source-supported inference, not a GPUI internals diagnosis.

### B8 — P2: wide Solo prose becomes a very long reading line

**Scenario:** Read an ordinary explanatory answer in a 1440×900 Solo window.

**Current behavior:** The answer row uses `w_full` and its text child `flex_1` with no reading-width bound (`crates/ferrite/src/transcript.rs:310`, `:336`). The inspected first paragraph's first line has **170 characters including spaces** in `prose-desktop.png`; the same paragraph's first line has **93** at 1000×800. Text is 13px. This is measured from the actual wrapped fixture, not a speculation about unbroken strings.

**Impact:** Wide-window reading needs long horizontal scans and long returns to the next line. Widening Solo makes ordinary prose less comfortable even when no content clips.

**Recommendation:** Establish a reading measure for prose in wide Solo while preserving appropriate width for code, tables and operational rows. Check the result with the same paragraph at both daily sizes.

**Confidence:** High for rendered measure and source; usability priority is a design judgment. Raw line transcriptions/counts are in `/tmp/ferrite-ui-prose-measure.json`.

### B9 — P2: Settings repeats its category heading within the content column

**Scenario:** Open New Threads or another Settings category.

**Current behavior:** “New Threads” appears as the sidebar selection, the content page title and again as a section title directly beneath it. Both content titles are visible in the native captures. All four Settings groups are named in `crates/ferrite/src/cockpit.rs:2615`, and each is wrapped in a page with the same title at `crates/ferrite/src/prefs.rs:131`. The toolkit draws the page title (`gpui-component-0.6.0/src/setting/page.rs:176`) and group title (`setting/group.rs:87`).

**Impact:** Two apparent levels convey identical information and occupy vertical space above the actual controls. This makes a small Settings panel feel assembled from nested defaults rather than a considered hierarchy.

**Recommendation:** Keep one content heading per category; reserve additional group headings for real subdivisions.

**Confidence:** High, code and both daily-size Settings framebuffers.

## Control-state measurements and implementation consistency

These measurements supplement the findings rather than automatically creating further defects.

| State | Actual role/color composition | Ratio / effect |
|---|---|---|
| Normal reading | `TEXT_2 #a8a8a8` on `PANE #171717` | 7.539:1 |
| Normal muted transcript | `#959595` on `#171717` | 5.985:1 |
| Muted nav recency | `#959595` on `NAV #232323` | 5.247:1 |
| Same muted text on selected/hovered selection | `FILL #343434` / `FILL_HOVER #3b3b3b` | 4.156:1 / 3.740:1; selected fill weakens metadata contrast |
| Full-opacity Pane focus | `#5e5e5e` on `PANE` | 2.765:1, 1px |
| Toolkit control focus | 50% `#5e5e5e` on `MENU` | 1.490:1, 3px outer ring |
| Toolkit ghost hover on MENU | 80% of `RAISED` lightened 10% over MENU | about `#2b2b2b`, 1.044:1 relative to surrounding ground |
| Toolkit ghost pressed on MENU | 80% of `RAISED` lightened 20% over MENU | about `#2e2e2e`, 1.092:1 relative to ground |
| Custom pointer selected-row hover | opaque `FILL_HOVER #3b3b3b` | Preserves a carried selected ground |
| Normal scrollbar | `#3a3a3a` on PANE / MENU | 1.576:1 / 1.296:1 |
| Hovered scrollbar thumb | `SEP #6e6e6e` on MENU | 2.891:1 |
| Status inks on PANE | Running / Decision / Blocked | 8.357:1 / 9.433:1 / 7.208:1 |
| Primary Question action | `GROUND #0e0e0e` on `TEXT #dedede` | 14.349:1; explicit hover, press, focus states |

**Exact weak-hover path:** Common controls call `.ghost()` at `crates/ferrite/src/components.rs:15`; Ferrite maps `theme.secondary = RAISED` at `crates/ferrite/src/theme.rs:768`. Installed toolkit `/Users/josephwylie/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-component-0.6.0/src/button/button.rs:1080` computes dark ghost hover from `secondary.lighten(0.1).opacity(0.8)` instead of using Ferrite's `secondary_hover`. Its hover application is `:623`–`:650`. The active counterpart at `button.rs:1133` uses `lighten(0.2).opacity(0.8)`. Own pointer styles use opaque explicit HOVER/FILL/FILL_HOVER/PRESSED in `crates/ferrite/src/pointer.rs:90`–`:115`. Ghost labels/icons carry their own color (`components.rs:24`; Settings gear `prefs.rs:242`), so inherited toolkit foreground changes do not add feedback. This explains subtle and inconsistent feedback between migrated toolkit buttons and hand-built controls.

**Selected Settings chip caution:** `prefs.rs:210` changes a normal background but never calls the toolkit's selected state. The toolkit installs normal hover handling because `self.selected` remains false (`button.rs:623`). A selected chip therefore has a source-level path to the same ghost hover background as an unselected chip; a hovered screenshot should confirm effective style precedence before treating that as a separate defect. It is not represented as runtime-confirmed here.

**Scrollbar limits:** `crates/ferrite/src/scrollbar.rs:26` has a 16px gutter and 48px minimum thumb length. Visual width is 6px at rest / 8px active (`:29`, `:33`). Hovering the gutter grows the thumb without brightening it (`:337`), while directly hovering the thumb brightens it (`:326`). Do not confuse the thin visible mark with a 6px-wide hit area.

## Typography, geometry and coverage positives

| Native element | Measured source value | Evidence |
|---|---|---|
| Chrome/meta/control labels | 11px, UI line height 1.45 | `theme.rs:235`, `:251`; `components.rs:26` |
| Transcript/tool text | 12px; body line height 1.55 | `theme.rs:228`, `:253` |
| Answer prose | 13px | `transcript.rs:316`; `theme.rs:232` |
| Markdown headings | 18 / 15.6 / 13.8px | `rich.rs:330` |
| Icon controls | 28px square, 16px glyph | `theme.rs:342`, `:567` |
| Settings choice | 22px high; horizontal padding 9px; gap 6px | `prefs.rs:25`, `:156`, `:207` |
| Project footer actions | 28px high | `project_editor.rs:209`, `:256` |
| Question actions | 32px high, 12px horizontal padding | `cockpit/subagents.rs:1860` |
| Tool disclosure | 20px glyph slot; full rendered header click area | `pane.rs:4950`; `transcript.rs:490` |
| Nav rows | Thread 41.75px, Group 43px | `theme.rs:367`, `:370` |
| Nav grouping | 16px between Groups, 6px before members, 2px siblings, 24px before solos | `theme.rs:374` |
| Settings card | 820×680px, capped to 94% width / 86% height; sidebar172px | `prefs.rs:21`, `:44`, `:76`, `:127` |
| Shared surface/control radii | 8 / 6 / 4px; modal10px | `theme.rs:454` |

The dense desktop scale is deliberate and not itself evidence of a defect. Native screenshots show useful heading/body separation, a clear visual distinction between answer prose and mono tools, coherent neutral surfaces, conspicuous primary Question actions, and readable normal body/status inks. Nav grouping uses multiple meaningful spacing levels; the prototype detector's monotonous-spacing result does not describe this production token system. Menus/choice controls use toolkit keyboard mechanics, and disclosure labels/metadata share their click target rather than forcing precise clicks on a tiny chevron. Settings remains fully within both tested ordinary window sizes. No blanket recommendation to apply mobile touch-target sizes, enlarge every label, add more borders, or replace the established visual identity follows from this pass.

## Suggested order

Repair B1 first, then address the focus/state composition in B2–B3 and completed-state reading in B4. B5–B9 are smaller, separately reviewable quality improvements. Confirm toolkit selected-hover precedence with one focused interaction capture before adding it as an independent ticket. The detector's two prototype warnings should not become native product tickets.
