# Ferrite UI quality audit — navigation, titlebar, Settings and Project editor

Source: product source at `fbc03f3` (current audit HEAD `6ae1e1b`; verified no diff in the audited UI files). No product changes. This is the presentation pass, independent of the earlier functional audit.

Evidence inspected: fresh native framebuffers `/tmp/ferrite-ui-root-fixtures/settings-desktop.png`, `settings-compact.png`, `prose-desktop.png`; `/tmp/ferrite-ui-baseline/formatting-wide.png`, `live-wide.png`, `decision-narrow.png`. Settings/prose captures are at ordinary 1440×900 and 1000×800 logical window sizes (2× backing). The taller baseline fixtures were used only to inspect the actual chrome. Empty transcript height is fixture content, not a finding.

## Confirmed visual findings

### NAVQ-01 — P2: Settings repeats each page name as a second section heading

**Visible behavior:** In both fresh Settings captures the content column starts with **New Threads**, then repeats **New Threads** immediately before Provider. The selected sidebar row also says New Threads, appropriately. The two content headings create a false extra level and spend the strongest heading position on information already stated.

**Why it feels unfinished:** The Settings screen visibly exposes the library's page-plus-group nesting instead of presenting a deliberate hierarchy. An operator scanning for Provider/model must first parse two titles that mean the same thing.

**Evidence:** `crates/ferrite/src/cockpit.rs:2615`–`:2618` give each sole group the page's name. `crates/ferrite/src/prefs.rs:129`–`:136` wrap each in a same-named SettingPage. Visually confirmed in `settings-desktop.png` and `settings-compact.png`; code establishes the same pattern for Permissions, Behaviour and About.

**Precise correction:** Keep the selected page heading and remove the redundant sole-group heading. If meaningful subgrouping is useful, use real groups such as Claude and Codex, with model and effort together. Preserve the existing compact text scale and dark surfaces.

### NAVQ-02 — P2: Settings’ option controls lose their visual grouping and look like loose labels

**Visible behavior:** Provider, model and effort values are rows of small text. Only the selected value has a visible chip surface. The unselected options disappear into the modal background, so model names and effort levels resemble a line of labels more than a mutually exclusive control. This is particularly evident in the six Codex model values in the fresh captures, where no selected value is drawn.

**Why it matters:** Users have to infer which text is interactive and that choosing one value replaces the others. The repeated rows also lack a stable visual boundary between the control and nearby helper copy. This happens with the ordinary shipped option labels, not with long input.

**Evidence:** `crates/ferrite/src/prefs.rs:25` fixes chip height to 22px; `:152`–`:158` lay options out as independent wrapping siblings; `:203`–`:214` give unselected chips RAISED and selected chips FILL, with no radio/check marker or common control container. `crates/ferrite/src/theme.rs:43` and `:48` make RAISED and MENU the identical `#282828`. Base buttons are ghost buttons (`crates/ferrite/src/components.rs:13`–`:19`). Fresh `settings-desktop.png` and `settings-compact.png` confirm the flattened appearance.

**Precise correction:** Present short effort ladders as a compact segmented group with one shared inset surface and a clearly indicated selected segment. Present the longer model catalog as a labeled compact select or grouped radio list with a check. Aim for 26–28px desktop control height while keeping 11–12px labels if matching the current density. Preserve the palette; this does not require brighter colours everywhere. Ensure the effective/default model always has a visible selected representation; the separate model-default functional issue should be deduplicated with the earlier audit.

### NAVQ-03 — P2: Two identical unlabelled plus buttons conceal different creation scopes

**Visible behavior:** The normal Solo screen has one plus beside All Projects and a second at the far right of the window titlebar. Both use the same glyph, size and muted treatment. Nothing visible distinguishes their meanings before hover.

**Why it matters:** The sidebar plus creates a loose Thread; when an ungrouped Thread is focused, the titlebar plus creates a Group with a new Thread. The principal Group-creation affordance therefore looks like a duplicate New Thread button. A new operator must discover the distinction through a tooltip or by invoking it. This presentation finding is distinct from the earlier broken Group context-menu handler.

**Evidence:** Visually confirmed in fresh `prose-desktop.png` and `formatting-wide.png`. `crates/ferrite/src/nav.rs:438`–`:445` and `crates/ferrite/src/titlebar.rs:121`–`:137` use the same PLUS/ICON_BUTTON treatment. The titlebar's differing labels/behaviour exist only in tooltips/handlers at `crates/ferrite/src/cockpit.rs:6682`–`:6705`.

**Precise correction:** Keep sidebar + as New Thread. Give the contextual titlebar action a visible, compact scope: e.g. the existing Group mark plus “New Group” in Solo, and Group mark plus “Add Thread” when viewing a Group. A short 70–100px labelled control fits the ordinary desktop titlebar and makes the product's central grouping capability discoverable without adding a toolbar.

### NAVQ-04 — P3: Settings’ bottom-left corner is square while the other modal corners are rounded

**Visible behavior:** Both fresh Settings screenshots show the sidebar reaching a hard square bottom-left corner; the right edge keeps the card's rounded bottom-right corner. It is visible on normal content at both window sizes.

**Why it feels unfinished:** One simple floating surface presents two conflicting edge treatments, drawing attention to the nested toolkit component rather than a coherent card.

**Evidence:** `settings-desktop.png` and `settings-compact.png`, bottom-left of modal. Outer card has `.overflow_hidden().rounded(R_MENU)` at `crates/ferrite/src/prefs.rs:52`–`:56`; its inner Settings sidebar has an opaque MENU background at `:123`–`:128` and fills the remaining body at `:139`. The exact GPUI clipping mechanism causing the child paint is not proven by this read-only pass.

**Precise correction:** Ensure the bottom-left Settings body/sidebar background uses the card's matching lower-left radius or a clipping container that masks descendants to the card's rounded shape. Confirm all four corners on the native framebuffer; do not adjust the overall modal radius or shadow style.

## Source-confirmed presentation findings awaiting matching capture

These are actionable source findings, but the screenshots supplied for this pass do not show their target states. They should remain labelled source-only unless the lead auditor adds a fresh matching capture.

### NAVQ-05 — P2: Main is represented by an anonymous horizontal dash beside named subagent tabs

**Scenario:** A Thread has subagents; an operator selects one and wants to return to Main.

**Current presentation:** The Main tab is a 24px-wide tab whose only visible child is a 17×2px rounded horizontal bar. Subagent tabs have names. The bar's meaning is only provided by hover tooltip/ARIA text, so it resembles a divider, minus or decoration more readily than the parent transcript destination.

**Impact:** The most fundamental return destination has the least recognizable label. Users must discover and remember a custom mark to recover their own conversation/composer.

**Evidence:** `crates/ferrite/src/cockpit/subagents.rs:380`–`:397` (Main tab and dash), contrasted with named child tabs at `:415`–`:420`. Source-confirmed, no matching fresh subagent framebuffer inspected by this agent yet.

**Precise correction:** Use a short visible **Main** label in the same tab grammar as subagents, retaining an optional small parent glyph. Prioritize keeping Main and the selected subject visible before spending space on other child tabs; avoid a larger new header row.

### NAVQ-06 — P2: Create/Done in the Project editor has no primary-action treatment

**Scenario:** First-run New Project after selecting a directory, or Edit Project after changing its name.

**Current presentation:** The helper called `primary_button` inherits the same ghost/xsmall base as the secondary Add Directory action, has no fill or border, and differs mainly by TEXT_STRONG versus TEXT_2. Its own comment describes “the one filled control,” but no filled treatment is implemented.

**Impact:** The action that completes setup or commits the Project name does not stand out from the card's supporting controls. The first-run surface requires scanning for the completion verb instead of offering a clear endpoint.

**Evidence:** `crates/ferrite/src/project_editor.rs:200`–`:214` (primary), `:252`–`:258` (secondary); `crates/ferrite/src/components.rs:13`–`:19` (ghost base). No matching fresh Project-editor capture inspected yet; precise resting appearance is inferred directly from the style definitions, not claimed as observed.

**Precise correction:** Give Create/Done the existing app's restrained filled primary-button treatment and a 28–30px height, keep Add Directory ghost, and keep Remove visually secondary/destructive. Use the same treatment that already makes “Send answer” readable as a primary action in the fresh Decision screenshot, without changing the modal layout.

## Positives / leave intact

- The current dark visual world is coherent: navigation is distinctly lighter than the content field; the active Thread has a clear, restrained selected surface.
- Sidebar action alignment, inset rhythm and two-line Thread row hierarchy look deliberate in the fresh captures. No speculative long-title issue is reported.
- Native Settings category navigation and a real Search field give a clear starting point; the panel remains comfortably inside both ordinary window sizes.
- The titlebar's location starts on the same left edge as the Pane board, and chrome actions retain a consistent small desktop target grammar.
- Project creation's empty-directory copy names the next action, and Create is disabled until a directory exists; preserve that guidance while strengthening the action treatment.
- The modal veil, overall shadow and normal top corners provide sufficient separation from the busy underlying cockpit. No heavier borders or redesign needed.

## Limits

No GUI control, product edits or additional agents in this subtask. Fresh screenshots were inspected as provided by the lead auditor; no historical screenshot is used as current proof. Hover/focus/motion behavior and Windows titlebar were not independently exercised. Functional findings from the first audit (creation scope bugs, Provider default behavior, Project retargeting, and partial save semantics) are not repeated as new quality findings. No P1 presentation-only issue was established in this scope; priorities are intentionally P2/P3.
