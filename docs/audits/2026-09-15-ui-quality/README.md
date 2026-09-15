Method: dual-agent (A: `/root/transcript_reading` · B: `/root/settings_platform`), plus three specialist reviewers and lead synthesis.

# Ferrite UI polish and quality audit — 15 September 2026

**Ferrite has a coherent visual identity, but its finish is inconsistent in ordinary use.** Its dark surfaces, compact desktop controls, distinct prose/tool typography and restrained status colors fit a multi-agent cockpit. The biggest opportunity is to make ownership, reading and action states equally deliberate: a normal Question must stay attached to its Thread, a compact overview must show whole readable lines, and keyboard focus must be visible.

**22 findings: 3 P1 major, 14 P2 material refinements, 5 P3 finishing details.** This is an audit extension on `audit/full-ux-2026-09-14`, against product baseline `fbc03f3`. No product code or UI was changed. Documentation, unmodified synthetic screenshots, computed evidence and unapplied capture patches are the deliverables. The [earlier 42-finding functional audit](../2026-09-14-full-ux/README.md) remains separate; this report does not recast its data-loss, routing or lifecycle issues as polish findings.

Open the [screenshot gallery](gallery.html), [full findings with source references and acceptance criteria](findings.md), or [evidence and reproduction notes](evidence/README.md).

## First five priorities

1. **P1 — Keep Questions inside their owning Pane.** A normal two-option Question at 1200×800 covers its Thread title and app titlebar. Constrain the whole island to available Pane space and scroll its options, preserving ownership and the action row. [UI-01](findings.md#ui-01), [screenshot](screenshots/question-group-1200x800.png). Suggested follow-up: `$impeccable harden`.
2. **P1 — Render complete text rows in the compact grid.** At 860×500, ordinary four-Thread content is cut horizontally through glyphs. Spend the height on the newest complete useful rows and omit older rows whole. [UI-02](findings.md#ui-02), [screenshot](screenshots/group-l2-860x500.png). Suggested follow-up: `$impeccable adapt`.
3. **P1 — Paint the keyboard target on tool disclosures.** The Tab/Enter path moves focus but the targeted control adds no visible styling. Show a distinct focused header before Enter opens it. This is source confirmed; a Tab sequence was not captured. [UI-20](findings.md#ui-20). Suggested follow-up: `$impeccable harden`.
4. **P2 — Make the Composer's primary actions discoverable.** Send/newline conventions are missing from the visible instructions, and interrupt is passive text. Add compact, neutral actions and clear composing guidance; keep model/effort secondary. [UI-03](findings.md#ui-03), [draft](screenshots/draft-1000x800.png). Suggested follow-up: `$impeccable clarify`.
5. **P2 — Give Settings a considered control hierarchy.** Remove the repeated content heading, group provider settings, expose current model values clearly and give effort choices a shared control boundary. Its subtle native focus/hover states need verification alongside the resting redesign. [UI-04](findings.md#ui-04), [UI-09](findings.md#ui-09), [screenshot](screenshots/settings-compact.png). Suggested follow-up: `$impeccable polish`.

These are concrete proposed refinements, not an instruction to implement or a request for additional approval. The user's audit-only scope remains in force.

## Design health

The score is a qualitative assessment of inspected presentation and affordances, not an accessibility certificate, usability-study result or release-readiness metric. All ten heuristics apply. The independent design review scored its initial Solo/Settings/transcript scope **26/40**; lead synthesis lowers system-status and control/freedom by one point each after verifying the additional Group and focus evidence. This single-run refinement is not a trend or a product regression.

| # | Heuristic | /4 | Key evidence |
|---|---|---:|---|
| 1 | Visibility of system status | 2 | Current state competes with repeated history; focus covers an alert outline; completion dims usable controls. |
| 2 | Match with the real world | 3 | Coding vocabulary and file/tool presentation fit the audience; anonymous Main and creation scopes need clearer labels. |
| 3 | User control and freedom | 2 | Question ownership is obscured; keyboard disclosure targets are invisible; primary writing controls are difficult to discover. |
| 4 | Consistency and standards | 2 | Coherent palette, but Settings, Project actions and operational controls use inconsistent state/priority treatments. |
| 5 | Error prevention | 3 | Question choices and primary submit action are clear locally; unavailable settings should be indicated before selection. |
| 6 | Recognition rather than recall | 2 | Main dash, duplicate plus glyphs and missing Send guidance require interpretation or prior knowledge. |
| 7 | Flexibility and efficiency | 3 | Dense multi-pane and keyboard workflows suit experts; readable Solo scale and image detail inspection need attention. |
| 8 | Aesthetic and minimalist design | 3 | Product-specific and restrained; heading decoration, rhythm, redundant status and one modal corner weaken finish. |
| 9 | Error recognition and recovery | 2 | Errors are visible, but repeated failure decoration and dim review annotations complicate inspection. |
| 10 | Help and documentation | 2 | Useful contextual descriptions exist; key primary-action guidance is missing while interruption hints repeat. |
| | **Total** | **24/40** | **Sound visual foundation; material quality work remains.** |

Functional dimensions are scored only as far as this presentation review supports them. Consult the first audit for operational trust and safety defects; this score does not override them.

## What is working

- **An authored desktop cockpit.** The related dark surfaces, modest radii, sidebar hierarchy and compact row grammar fit sustained coding work. The interface does not need a replacement palette, gratuitous shadows or large decorative cards.
- **Useful text and status distinctions.** Human prompts, sans-serif answers and monospace tools are recognizable. Normal body and status inks have strong measured separation from their grounds; grey text is not inherently the problem.
- **A good local Question design.** The Question heading, radio labels, subordinate descriptions and bright Send answer button establish clear priority. Preserve that hierarchy while fixing containment. The ordinary L1 Group's seams and title bands are also coherent.

## Full quality index

| ID | Priority | Present-day issue | Evidence |
|---|---|---|---|
| [UI-01](findings.md#ui-01) | P1 | Question hides owning Thread and app titlebar | Framebuffer + source |
| [UI-02](findings.md#ui-02) | P1 | Compact Group slices ordinary text through baselines | Framebuffer + source |
| [UI-20](findings.md#ui-20) | P1 | Keyboard disclosure focus has no visible target | Source call path |
| [UI-03](findings.md#ui-03) | P2 | No visible Send/newline route; interrupt is passive text | Framebuffer, native app, source |
| [UI-04](findings.md#ui-04) | P2 | Flat Settings choices and duplicated hierarchy | Framebuffer, native app, source |
| [UI-05](findings.md#ui-05) | P2 | Model/effort look available during a guaranteed busy refusal | Closed control frame + source |
| [UI-06](findings.md#ui-06) | P2 | Suggested and entered prompt text share the same treatment | Source; placeholder frame |
| [UI-07](findings.md#ui-07) | P2 | Main tab is an anonymous dash | Framebuffer, native app, source |
| [UI-08](findings.md#ui-08) | P2 | Wide Solo increases eye travel without reading scale | Two ordinary window captures |
| [UI-09](findings.md#ui-09) | P2 | Settings hover/focus feedback is exceptionally subtle | Native toolkit source + calculation |
| [UI-10](findings.md#ui-10) | P2 | Completion fades an active Composer | Framebuffer + source |
| [UI-11](findings.md#ui-11) | P2 | Image preview has no detail-inspection escape hatch | Source only |
| [UI-12](findings.md#ui-12) | P2 | Code block boundaries and direct Copy affordance are weak | Framebuffer + source |
| [UI-13](findings.md#ui-13) | P2 | Identical plus glyphs conceal different creation scopes | Framebuffer + source |
| [UI-19](findings.md#ui-19) | P2 | Enabled Project completion inherits secondary ghost style | Source; disabled empty-state frame only |
| [UI-21](findings.md#ui-21) | P2 | Focus paints over an alert perimeter | Framebuffer + source |
| [UI-22](findings.md#ui-22) | P2 | Diff line numbers and omission counts use separator ink | Source + calculation |
| [UI-14](findings.md#ui-14) | P3 | H1 is unrequested italic/underlined; deep headings collapse | Framebuffer + source |
| [UI-15](findings.md#ui-15) | P3 | Failure decoration and Escape hints repeat | Framebuffer + source |
| [UI-16](findings.md#ui-16) | P3 | Short commentary gets disproportionate answer padding | Framebuffer + source |
| [UI-17](findings.md#ui-17) | P3 | Settings has one square lower corner | Two framebuffers + pixel check |
| [UI-18](findings.md#ui-18) | P3 | Focus removes the Composer gutter and shifts text | Source layout; static Group states |

## Reading, cognitive load and user roles

The calm initial impression is a strength. Reading fatigue comes from wide fixed-size prose and the abrupt spacing between short commentary and tools. Supervision gets harder when a historical failure has more decoration than the current state, or a completed Pane's input looks disabled. A Decision should be the reassuring high point; its local controls are clear, but the Group overlay can erase its owner.

Settings is the clearest choice-load problem: both model catalogs and both effort ladders show more than four alternatives per field at once, with little visual grouping. The remedy is clearer field structure and current values, not arbitrarily hiding necessary configuration.

- **Expert supervising four agents:** L2's sliced lines prevent quick reading; Question overflow hides the owner; invisible disclosure focus undermines keyboard operation. Fix these before polishing decorative details.
- **First-time operator:** Two plus signs imply different creation scopes, Main is an unexplained mark, and the Composer exposes model configuration more clearly than Send. Make primary nouns and actions recognizable.
- **Reader reviewing a substantial answer or screenshot:** Long 13px Solo lines, weak code-block affordances and fit-only image preview increase inspection effort. Offer usable reading/inspection controls while retaining compact defaults.

These are scenario-based review perspectives, not claims from recruited user research.

## Intent, exclusions and smaller decisions

The source explicitly records that the operator rejected a fixed 68ch prose cap. UI-08 respects that decision: improve reading scale or offer an explicit user-controlled treatment; do not reinstate the cap silently. The dense desktop scale and quiet neutral focus ring are deliberate. A 28px desktop icon control is not penalized against a phone touch-target checklist. L2/L3 retain their existing design; fixing sliced rows does not require inventing a new visual system.

Other smaller observations were not inflated into separate findings: table headers could use modest weight/column-alignment refinement; prompt/answer weight balance could improve; different menu widths are not inherently inconsistent. Stock attachment-card styling and reduced-motion-aware attachment entrance are existing intentional choices. The tool scrollbar has a larger hit area than its thin visible thumb. Long titles, empty space in the tall synthetic fixtures and transient overlapping toasts from an immediate capture were not counted as defects.

The useful design decisions for later implementation are: the precise neutral primary-action treatment; a readable Solo scale that respects full width; and one combined focus/alert recipe. No clarification was needed to finish this audit, and no implementation work was started.

## Method, independence and verification boundary

The same **five user-authorized subagents** covered (1) independent visual design, (2) independent native measurements and detector scope, (3) navigation/Settings/Project presentation, (4) Composer/control states, and (5) Group/Decision/status captures. Assessment A finished and recorded its verdict before Assessment B's detector or measurement conclusions entered lead synthesis. The lead checked sources and screenshots, merged overlap, calibrated severity and preserved uncaptured-state limitations. Original reviewer notes are [archived](evidence/reviews/README.md); the consolidated findings above are authoritative where priorities differ.

The native capture harness generated 24 current screenshots across baseline component states and everyday windows; **17 selected, unmodified PNGs** are retained. Everyday captures cover 1440×900 and 1000×800 Solo/Settings, 1200×800 four-Thread Group/Question, 860×500 L2, draft, first-run Project and subagent tabs. PNG backing scale is 2×. Fake Sessions and disposable stores exercise the actual renderer; fixture command failures are synthetic, not failures of real tests or user work. Only temporary copies of the capture harness changed. Both custom capture builds completed successfully; their patches and logs are retained.

The installed macOS app's two-pane Group and three Settings categories were also inspected without changing preferences or taking agent actions. Its exact compiled SHA is unknown, so reproducible current-source captures carry more evidentiary weight. Windows rendering, real provider transitions, image zoom, a full keyboard traversal, frame pacing, motion playback and sustained performance were not runtime validated. No broad product test suite was rerun because there is no product patch.

**Deterministic scan:** The required `detect.mjs --json crates/ferrite/src` attempt returned two warnings: `flat-type-hierarchy` at `nav-project-sections.prototype.html:38` and `monotonous-spacing` on the same prototype. Its inventory contains two HTML prototypes and **zero Rust files**. Both warnings are outside production scope; neither is promoted to a native defect. Independent native measurements support specific findings such as faint toolkit focus, while ordinary text/status contrast supports preserving the palette. The detector provides no native cleanliness verdict.

A browser DOM overlay cannot instrument a native GPUI surface. Native computer-use inspection and framebuffer evidence replace that inapplicable visualization; no overlay or local web server was started. Ratios are computed from source tokens and modeled opacity, not a complete accessibility certification or pixel-perfect assertion for every GPU state.
