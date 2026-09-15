# UI polish and quality findings

These 22 findings extend the functional audit. They are ranked by present-day impact: **3 P1 major, 14 P2 material refinements, 5 P3 finishing details**. P1 prevents ordinary reading or obscures action ownership; P2 adds recurring interpretation or interaction effort; P3 is a smaller consistency or finish issue. There is no P0 finding. Proposed treatments are recommendations, not implemented changes or usability-test results.

**Evidence:** F = fresh production-renderer framebuffer with synthetic content; V = installed native app observation; S = source-confirmed. A screenshot can establish appearance without establishing every interaction. Source-only transitions and missing capture states are called out below. See the [gallery](gallery.html) and [reproduction notes](evidence/README.md).

<a id="ui-01"></a>
## UI-01 · P1 · A normal Question covers its owning Thread and the app titlebar

**Trigger and impact:** Receive a two-option Question in a four-Thread Group at 1200×800. The top-left Question rises above the Pane, covers its title and overlaps the application breadcrumb/titlebar. A user answering several agents loses the most direct visual evidence of which Thread owns the request. Short normal labels reproduce it.

**Evidence — F/S:** [Question in a Group](screenshots/question-group-1200x800.png). [subagents.rs:1456](../../../crates/ferrite/src/cockpit/subagents.rs#L1456) caps only the content using the whole window height; the island adds its own heading, footer and padding. [pane.rs:2822](../../../crates/ferrite/src/pane.rs#L2822) puts the request in a deferred absolute layer above the Composer, outside Pane clipping. This is distinct from UX-04's compact-grid keyboard failure.

**Refinement and acceptance:** Constrain the whole island to its owning Pane's available rectangle, retaining the Thread identity and action row; scroll its options internally. At 1200×800, all four Thread headers and the app titlebar must remain visible when this exact Question appears. If a deliberate expanded review state is needed, keep the owning Thread explicitly named.

<a id="ui-02"></a>
## UI-02 · P1 · The compact grid slices ordinary text through its baselines

**Trigger and impact:** View the same four Threads at 860×500, where Ferrite uses Instruments/L2 presentation. Several prompt, answer and failure rows become horizontal slivers. The overview shows fragments of multiple unreadable rows instead of one useful latest update. This is vertical layout failure with short ordinary content, not long-string ellipsis.

**Evidence — F/S:** [Compact Group](screenshots/group-l2-860x500.png). [pane.rs:1654](../../../crates/ferrite/src/pane.rs#L1654) inserts a tail into the remaining height; [pane.rs:1686](../../../crates/ferrite/src/pane.rs#L1686) combines a shrinking flex column, bottom justification and clipping. Child rows clamp lines but do not preserve an intact row height under this pressure.

**Refinement and acceptance:** Budget the available height for complete semantic rows. Prefer the newest meaningful update or failure, omit older rows as whole rows, and preserve line height. The exact captured Group must show readable complete lines in all four Panes; headers, status and Composer must not compete by shrinking transcript glyphs. This is a correctness fix within the existing L2 design, not a proposed L2 redesign.

<a id="ui-03"></a>
## UI-03 · P2 · The Composer does not reveal how to send, insert a newline, or click Stop

**Trigger and impact:** Enter a draft or supervise a running Thread. Model and effort have visible dropdown affordances, but the primary writing action has no Send control or Enter/Shift+Enter instruction. The visible `esc interrupt` text is not clickable. First-time and pointer-oriented users must already know the submission convention while secondary configuration is easy to discover.

**Evidence — F/V/S:** [Draft](screenshots/draft-1000x800.png), [working Thread](screenshots/subagents-1000x800.png), [pane.rs:2669](../../../crates/ferrite/src/pane.rs#L2669), [hints at 2866](../../../crates/ferrite/src/pane.rs#L2866), and [plain interrupt text at 2772](../../../crates/ferrite/src/pane.rs#L2772). Existing keyboard actions work; this finding concerns affordance. The earlier interrupt/queue behavior issue remains separate.

**Refinement and acceptance:** Add a compact neutral Send action, clear composing guidance for Enter and Shift+Enter, and a real interrupt action in the same control family. Preserve keyboard speed. A new user should be able to identify these three actions without guessing or hovering over unrelated controls. Lead severity is P2 rather than the controls reviewer's P1 because the familiar keyboard path remains available.

<a id="ui-04"></a>
## UI-04 · P2 · Settings flattens current values, alternatives and headings

**Trigger and impact:** Open New Threads. The content says “New Threads” twice, then presents both model catalogs and effort ladders as numerous similar small text chips. Unselected values look like passive labels. The reader must reconstruct field boundaries and current choices instead of scanning defaults first.

**Evidence — F/V/S:** [Settings at 1000×800](screenshots/settings-compact.png). [prefs.rs:129](../../../crates/ferrite/src/prefs.rs#L129) wraps already titled groups in same-named pages; [cockpit.rs:2615](../../../crates/ferrite/src/cockpit.rs#L2615) names those groups. [prefs.rs:203](../../../crates/ferrite/src/prefs.rs#L203) uses 22px independent chips whose unselected RAISED ground equals the MENU ground. Measurement confirms the selected fill is only 1.18:1 against the modal, although its brighter text is visible. Missing Codex-default representation is already UX-39, not another new issue here.

**Refinement and acceptance:** Keep one page heading; group each provider's model and effort together. Use a selected-value chooser for the model catalog and a clearly bounded segmented group for effort, with an explicit selected cue. Both providers may remain configurable. At a glance, the user should distinguish field label, explanation, current value and available alternatives. No palette replacement is needed.

<a id="ui-05"></a>
## UI-05 · P2 · Model and effort look available while the core already knows they are unavailable

**Trigger and impact:** While a Thread runs, open model or effort and choose a value. The controls and rows look enabled, yet the core rejects the change as busy. The menu closes and feedback appears elsewhere, making an ordinary selection feel unreliable.

**Evidence — F/S:** [Working Group](screenshots/group-l1-1200x800.png) establishes the ordinary resting appearance. [cockpit.rs:4435](../../../crates/ferrite/src/cockpit.rs#L4435) makes model rows non-fixed; [effort rows at 8031](../../../crates/ferrite/src/cockpit.rs#L8031) are similarly active. Busy refusals are explicit in [core cockpit.rs:2787](../../../crates/ferrite-core/src/cockpit.rs#L2787) and [2854](../../../crates/ferrite-core/src/cockpit.rs#L2854). The open-menu refusal was source traced, not exercised against a live provider.

**Refinement and acceptance:** Show unavailable choices with a concise “Available when this turn finishes” explanation, preserving the current selection and predictable focus. This need not change core behavior or queue settings changes. A working Thread should never visually invite a choice that is guaranteed to be refused for its already-known state.

<a id="ui-06"></a>
## UI-06 · P2 · A suggested follow-up and the user's actual draft have the same ink

**Trigger and impact:** A completed Thread offers a predicted follow-up; Tab accepts it into the input. Ghost and real text use the same face, size and TEXT_2 color. Acceptance has little visual confirmation, so the user has to infer whether words are merely suggested or actually part of the draft.

**Evidence — S, placeholder corroborated by F:** [pane.rs:2644](../../../crates/ferrite/src/pane.rs#L2644) establishes typed text styling and [2690](../../../crates/ferrite/src/pane.rs#L2690) draws the overlay with identical ink. [cockpit.rs:3324](../../../crates/ferrite/src/cockpit.rs#L3324) accepts the suggestion. The before/after suggestion transition was not captured. No automatic sending is alleged.

**Refinement and acceptance:** Keep ghost text readable but visibly subordinate; promote entered or accepted text to the primary text token and retain the explicit Tab hint. A side-by-side capture before and after acceptance should reveal the state difference without relying solely on caret position.

<a id="ui-07"></a>
## UI-07 · P2 · Main is an anonymous dash beside named subagent tabs

**Trigger and impact:** A Thread gains subagents. The parent transcript becomes a 17×2 horizontal bar, while its children have names. The return destination resembles a minus sign or divider. Users must learn a private symbol to find their own conversation.

**Evidence — F/V/S:** [Subagent tabs](screenshots/subagents-1000x800.png), [subagents.rs:380](../../../crates/ferrite/src/cockpit/subagents.rs#L380). The tooltip and accessibility label say Main transcript, but neither is a persistent visible label. This is separate from UX-36's changing owning title.

**Refinement and acceptance:** Render “Main” using the same tab grammar as children. Keep Main and the selected subject visible ahead of additional child tabs. Users should be able to name the return destination from a static screenshot without a tooltip.

<a id="ui-08"></a>
## UI-08 · P2 · Expanding Solo lengthens eye travel without improving reading scale

**Trigger and impact:** Read an ordinary explanation in a 1440×900 Solo window. The answer remains 13px while the first paragraph spans almost the full Pane; the captured first line contains about 170 characters. More window space mainly creates longer eye movements and harder line returns.

**Evidence — F/S:** Compare [desktop prose](screenshots/prose-desktop.png) with [compact prose](screenshots/prose-compact.png). [rich.rs:241](../../../crates/ferrite/src/rich.rs#L241) uses full width; [theme.rs:232](../../../crates/ferrite/src/theme.rs#L232) sets answer size. **Existing product intent matters:** [pane.rs:4316](../../../crates/ferrite/src/pane.rs#L4316) explicitly records the operator's rejection of a 68ch cap. Full-width prose is deliberate, not an accidental regression.

**Refinement and acceptance:** Respect that decision. Provide a readable Solo text-size adjustment or another user-controlled reading treatment, preserving dense Group defaults and full-width code/diffs. An optional width mode would require a product choice; do not silently reinstate the rejected cap. Acceptance should compare the same paragraph at both ordinary window sizes with the chosen reading setting. Lead severity is P2 because this is a design refinement with an explicit existing constraint, not unreadable or lost content.

<a id="ui-09"></a>
## UI-09 · P2 · Small Settings controls give exceptionally weak hover and focus feedback

**Trigger and impact:** Move the pointer or keyboard focus among Settings' already low-affordance options. The native ghost-button theme barely changes against the modal. Users get little additional evidence of which small control is targeted.

**Evidence — S/measurement:** [Measurement report](evidence/measurements.json) computes native toolkit ghost hover as approximately #2b2b2b over #282828 (1.04:1), and the half-opacity focus outline as approximately #434343 over #282828 (1.49:1). [components.rs:13](../../../crates/ferrite/src/components.rs#L13) selects ghost buttons; [prefs.rs:203](../../../crates/ferrite/src/prefs.rs#L203) uses them for choices; Ferrite's toolkit token mapping is in [theme.rs](../../../crates/ferrite/src/theme.rs). These are computed state colors, not claims of a captured full keyboard traversal or a compliance certification. The focus outline's width and shape also contribute to visibility.

**Refinement and acceptance:** Give these controls a clearly visible neutral keyboard-focus outline and a distinguishable hover surface. Keep focused Pane styling separate: its deliberate subtle 1px ring need not be brightened globally. Capture resting, hovered, selected and keyboard-focused versions on the actual modal and verify each state independently.

<a id="ui-10"></a>
## UI-10 · P2 · Completed Panes make their active Composer look disabled

**Trigger and impact:** A Thread completes in L2. Ferrite fades the whole Pane, including the input that still accepts follow-up work. Completion should make review and continuation easy to recognize; here it visually suppresses the next action.

**Evidence — F/S:** Bottom-left of [compact Group](screenshots/group-l2-860x500.png). [pane.rs:1666](../../../crates/ferrite/src/pane.rs#L1666) assembles the Composer with other content and then applies [DONE_CELL_OPACITY 0.75](../../../crates/ferrite/src/theme.rs#L673) to everything. “done,” “turn complete,” and “idle” also repeat the same state.

**Refinement and acceptance:** Keep interactive controls at their normal contrast and consolidate completion into one clear indicator. De-emphasize only secondary historical content. The completed Pane's input should look as available as the working Pane's input, while their states remain easy to distinguish.

<a id="ui-11"></a>
## UI-11 · P2 · Image preview lacks a usable detail-inspection path

**Trigger and impact:** Open a screenshot of code or an error dialog from an attachment in a Group. The preview contains the entire image inside a fraction of the Pane; the only action is Close. A normal desktop screenshot can become too small to verify its text, with no 100%, zoom or Open Original escape hatch.

**Evidence — S only:** [attachment_preview.rs:113](../../../crates/ferrite/src/attachment_preview.rs#L113) sets 90% Pane width and 85% height, capped at 48rem; [151](../../../crates/ferrite/src/attachment_preview.rs#L151) permanently uses contain. The header contains only Close. No image-preview capture was produced, so the exact visual severity remains to be checked with a representative screenshot.

**Refinement and acceptance:** Preserve fit-to-Pane as the default; add Open Original and, if supported, Fit/100% with panning when zoomed. A user should be able to inspect ordinary screenshot text before sending it. This is an inspection affordance recommendation, not an allegation of damaged image data.

<a id="ui-12"></a>
## UI-12 · P2 · Fenced code lacks a clear block surface and direct Copy action

**Trigger and impact:** Read and reuse the ordinary Rust example in an answer. The code uses the same background as prose, zero block inset, no language label and no Copy control. Typography distinguishes it, but its boundaries and reusable-unit affordance are weak compared with the surrounding structured tool UI.

**Evidence — F/S:** [Prose and code](screenshots/prose-desktop.png), [formatting](screenshots/formatting-wide.png). [rich.rs:310](../../../crates/ferrite/src/rich.rs#L310) chooses PANE and zero padding; [265](../../../crates/ferrite/src/rich.rs#L265) returns no actions for non-HTML code. [Vendor code rendering](../../../vendor/gpui-base/src/text/node.rs#L1293) adds only the supplied actions, with no hidden default Copy button. Manual text selection/copy exists and is not described as broken.

**Refinement and acceptance:** Use a restrained inset or edge, a compact language label when available, and a Copy action with confirmation. Preserve exact whitespace and horizontal code space. The code block should read as one reusable unit without a large toolbar or decorative card.

<a id="ui-13"></a>
## UI-13 · P2 · Identical plus buttons conceal different creation scopes

**Trigger and impact:** In Solo, inspect the plus beside All Projects and the plus at the far right of the titlebar. One creates a loose Thread; the other starts a Group with a new Thread. Their identical glyphs give no visible explanation of this important difference before hover.

**Evidence — F/S:** [Solo](screenshots/prose-desktop.png). [nav.rs:438](../../../crates/ferrite/src/nav.rs#L438), [titlebar.rs:121](../../../crates/ferrite/src/titlebar.rs#L121), and [context-dependent behavior at cockpit.rs:6682](../../../crates/ferrite/src/cockpit.rs#L6682). Tooltips do describe the actions; the issue is first-look discovery, separate from UX-18's incorrect Group context-menu behavior.

**Refinement and acceptance:** Keep the sidebar creation action compact; visibly label the contextual titlebar action “New Group” in Solo and “Add Thread” in a Group, using the existing Group mark if helpful. The different scopes should be understandable without invoking either action.

<a id="ui-14"></a>
## UI-14 · P3 · Markdown heading decoration is inconsistent with the rest of the interface

**Trigger and impact:** Render ordinary `# Heading one`. It becomes bold, italic and underlined, while H2 uses a more restrained heading treatment; H3 through H6 share one size. The highest heading looks link-like and over-decorated, and deeper hierarchy collapses.

**Evidence — F/S:** [Formatting capture](screenshots/formatting-wide.png); the [input fixture](../../research/cli-capture-2026-09-06/model-source/claude-formatting.md) contains plain heading syntax. [Vendor node.rs:2425](../../../vendor/gpui-base/src/text/node.rs#L2425) adds italic and underline to every H1; [rich.rs:330](../../../crates/ferrite/src/rich.rs#L330) assigns one size to H3–H6.

**Refinement and acceptance:** Give H1 a clean weight/size hierarchy without unrequested italic/underline, reserving link styling for links. Differentiate deeper levels modestly through weight or spacing if needed. Test plain headings and explicitly authored emphasis separately so author intent is preserved.

<a id="ui-15"></a>
## UI-15 · P3 · Repeated status decoration competes with the current state

**Trigger and impact:** A tool fails during a turn. The same event receives a red group label, “1 failed” badge, red child verb, another “failed” badge and red detail; nearby current/completed status is quieter. Running surfaces also repeat “esc to interrupt” and “esc interrupt” on adjacent rows. The screen spends emphasis on repetition.

**Evidence — F/S:** [Working and failed Group](screenshots/group-l1-1200x800.png), [subagent/Composer hints](screenshots/subagents-1000x800.png). [Group failure rendering](../../../crates/ferrite/src/pane.rs#L4676), [child badge](../../../crates/ferrite/src/pane.rs#L4480), [progress hint](../../../crates/ferrite/src/pane.rs#L2451), and [Composer hint](../../../crates/ferrite/src/pane.rs#L2764). Failures are synthetic fixture content, not a claim that a real test run failed.

**Refinement and acceptance:** Retain one clear failure summary plus the useful error explanation, and show the interrupt instruction once. Establish a stable current-state location. The user should readily distinguish a historical failed tool from a currently blocked or failed Thread. Do not hide errors or weaken their signal globally.

<a id="ui-16"></a>
## UI-16 · P3 · Short commentary receives the same heavy spacing as a substantial answer

**Trigger and impact:** Read a normal sequence of short commentary and tool calls. Each prose run receives 12px above and below plus the inter-block gap, while tool rows use about 1px vertical padding. The alternating rhythm breaks related work into disproportionately separate islands.

**Evidence — F/S:** [Group commentary](screenshots/group-l1-1200x800.png), [prose](screenshots/prose-compact.png). [transcript.rs:317](../../../crates/ferrite/src/transcript.rs#L317) applies answer padding; [theme.rs:596](../../../crates/ferrite/src/theme.rs#L596) and [615](../../../crates/ferrite/src/theme.rs#L615) set answer and block gaps.

**Refinement and acceptance:** Make spacing express relationships: tighter commentary/tool sequences and a clearer actual turn boundary. Retain readable paragraph gaps and the Ferrite mark. Compare a multi-step coding exchange, not just an isolated final answer. This is a rhythm judgment, not a complaint about the deliberately empty area in tall fixture windows.

<a id="ui-17"></a>
## UI-17 · P3 · Settings has one visibly square lower corner

**Trigger and impact:** Open Settings at either captured everyday window size. Its sidebar paints a square lower-left corner while the lower-right corner is rounded. This simple inconsistency makes the floating surface feel unfinished.

**Evidence — F/S:** [Compact Settings](screenshots/settings-compact.png), [desktop Settings](screenshots/settings-desktop.png). [prefs.rs:52](../../../crates/ferrite/src/prefs.rs#L52) rounds the outer card; [123](../../../crates/ferrite/src/prefs.rs#L123) gives its inner sidebar an opaque background. The exact child-clipping mechanism was not separately reproduced.

**Refinement and acceptance:** Match the lower-left body/sidebar radius to the card or use a descendant mask that respects the rounded shape. All four corners must be coherent in native captures. Keep the existing radius and shadow design.

<a id="ui-18"></a>
## UI-18 · P3 · Composer text shifts horizontally when its Pane gains focus

**Trigger and impact:** Switch focus between two Panes. The unfocused Composer has a leading `›` and 8px gap; focus removes their layout space. Draft text moves sideways at the moment the user starts editing, creating a small visual twitch.

**Evidence — S, static states visible in F:** [pane.rs:2708](../../../crates/ferrite/src/pane.rs#L2708), [Group](screenshots/group-l1-1200x800.png). The movement follows the conditional layout; an animated or paired identical-draft capture was not made. The exact distance depends on glyph metrics, so no measured 15px claim is made.

**Refinement and acceptance:** Reserve a stable gutter in both states and change the mark without removing its box. The same draft's text origin should remain fixed through focus changes, while the caret and focus indicator remain clear.

<a id="ui-19"></a>
## UI-19 · P2 · Project completion uses the same ghost treatment as secondary actions

**Trigger and impact:** Finish selecting directories for a New Project, or edit an existing Project. Create/Done inherits the secondary ghost-button treatment and differs chiefly in text brightness. The setup flow's endpoint has weak visual prominence.

**Evidence — S; disabled empty state only in F:** [project_editor.rs:200](../../../crates/ferrite/src/project_editor.rs#L200) describes a filled confirming button but gives it no fill/primary variant; [252](../../../crates/ferrite/src/project_editor.rs#L252) uses the same base for Add Directory. [New Project capture](screenshots/new-project-1000x800.png) shows the empty state, where Create is correctly disabled. It does **not** establish the enabled button's exact appearance; that conclusion comes from the source. The empty capture's Name control is also not used as an independent finding without interaction verification.

**Refinement and acceptance:** Use the existing restrained filled primary treatment for enabled Create/Done; keep Add Directory secondary. Preserve the disabled state until a directory exists. Capture empty, ready-to-create and editing states, and ensure the completing action is obvious in the latter two. Earlier partial-save semantics remain UX-28 and are not recounted here.

<a id="ui-20"></a>
## UI-20 · P1 · Keyboard focus on a tool disclosure has no visible target

**Trigger and impact:** In a normal L1 Thread with several tool or reasoning disclosures and no subagent/Question taking over Tab traversal, press Tab to cycle disclosures and Enter to open one. The targeted row gets keyboard focus without a corresponding painted change. The operator cannot see which row Enter will activate.

**Evidence — S only:** [cockpit.rs:3412](../../../crates/ferrite/src/cockpit.rs#L3412) cycles the target and focuses the disclosure handle; [transcript.rs:484](../../../crates/ferrite/src/transcript.rs#L484) passes the target state to the renderer. [pane.rs:4973](../../../crates/ferrite/src/pane.rs#L4973) adds only focus tracking/key context, with no background, ring or ink change; its preceding comment explicitly describes the missing visible ground. The lead checked this complete call path. A before/after Tab framebuffer was not produced, so this is a source-confirmed defect rather than a runtime reproduction claim.

**Refinement and acceptance:** Paint a visible keyboard target on the full disclosure header, distinct from hover and expanded state. Tab through three disclosures in both directions: the next Enter target must be identifiable before activation. Preserve the existing full-header pointer target. This is a new missing visual state, separate from the first audit's Decision focus-routing failures.

<a id="ui-21"></a>
## UI-21 · P2 · Focus paints over a Pane's Decision or blocked outline

**Trigger and impact:** Select a Pane waiting for input or showing blocked state. Its amber/red outline becomes neutral grey because the focus border occupies exactly the same pixels. The inner status dot and Decision card remain, but one channel for locating attention disappears when focus arrives.

**Evidence — F/S:** [Focused approval](screenshots/approval-narrow.png), [Question Group](screenshots/question-group-1200x800.png). [pane.rs:858](../../../crates/ferrite/src/pane.rs#L858) chooses a state edge; [pane_shell at 1058](../../../crates/ferrite/src/pane.rs#L1058) paints it. [focus_wrapper at 1073](../../../crates/ferrite/src/pane.rs#L1073) subsequently paints a neutral border over the same rectangle. This does not imply total loss of the Decision state.

**Refinement and acceptance:** Define an intentional combined focus-plus-alert treatment that retains both meanings without changing Pane geometry or adding a glaring global ring. Compare idle/focused, waiting/unfocused, waiting/focused, blocked/unfocused and blocked/focused states side by side. The selected alert Pane should still read as needing attention.

<a id="ui-22"></a>
## UI-22 · P2 · Useful diff annotations are styled as decorative separators

**Trigger and impact:** Inspect an expanded edit and locate its line numbers or the count of undisplayed lines. Those annotations are substantially dimmer than the changed code, although they carry useful review information.

**Evidence — S/measurement:** [pane.rs:5162](../../../crates/ferrite/src/pane.rs#L5162) and [5199](../../../crates/ferrite/src/pane.rs#L5199) apply SEP #6e6e6e to line numbers and omission counts. The [computed ratios](evidence/measurements.json) are 3.52:1 against the plain Pane and about 2.90–2.96:1 against added/removed washes; changed-code inks are about 8.83 and 7.53:1. These are role-specific color measurements, not a claim that all code is hard to read. No matching diff-state framebuffer is supplied in this extension.

**Refinement and acceptance:** Give meaningful line numbers and omission counts an appropriate metadata text token, preserving the quieter hierarchy relative to code. Reserve SEP for decoration. Check ordinary added, removed and context lines on the native renderer. The earlier indentation and filename defects remain UX-12/13; this finding concerns visible annotation quality.
