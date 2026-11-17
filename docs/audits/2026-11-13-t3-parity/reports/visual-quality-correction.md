# Browser and panel visual-quality correction

The browser's visual problem is concrete: the new shared panel header still contains a legacy preview card, its own Browser heading, its own page-tab row, and then its toolbar. The resulting hierarchy looks assembled from different interfaces. The panel can also keep the size demanded by another surface after the user returns to a sparse Overview or Browser. These are quality failures in existing features. Counting tabs, a toolbar, resizing, and error text as implemented does not establish that the browser or panel is finished.

This is an independent visual assessment for the audit correction, not a new feature-parity inventory or a request for a different visual identity. It uses the existing Ferrite palette, density, and shared-panel direction as its starting point. Proposed corrections concern integration, affordance, stable geometry, and actual task completion.

## Evidence and limits

Ferrite source: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 source: `74573fc29b901bafbf8f1dbb2eb29606da1fa448` in `/tmp/ferrite-t3-audit-source-20261113`. No application code, settings, or user data were changed by this worker. No application was controlled, model/provider thread started, build run, or test run by this worker.

I personally inspected these saved images:

| Image | What it establishes | What it does not establish |
| --- | --- | --- |
| [Overview first open](../evidence/panel-correction/02-overview-first-open.png) | Compact Overview, visible navigation, conversation and panel proportions. | Animated opening or any intermediate frame. |
| [Overview after Files](../evidence/panel-correction/05-overview-after-files.png) | The same sparse Overview occupies a much larger right region; navigation is collapsed and conversation is narrower. | The entire action sequence by itself; the runtime worker owns that sequence and measurements. |
| [Browser after Files](../evidence/panel-correction/13-browser-after-files.png) | Shared Browser tab, repeated blue Browser heading, inner plus row, address toolbar, inset rounded preview card, and unavailable-host error presentation. | Native webpage rendering or a failure of the installed browser. This headless host deliberately lacks CEF. |
| [Browser after Changes](../evidence/panel-correction/15-browser-after-changes.png) | The same browser hierarchy and error layout alongside another outer surface tab. | Native loading, address focus behavior, or native error recovery. |

The parent separately and personally viewed a loaded `example.com` page in the installed, browser-capable Ferrite at 1920 × 1050. The parent reports the app surface row at approximately y=0–43, the blue Browser heading at y=43–82, the nested `example.com` tab at y=82–120, the Back/Forward/Reload/Annotate/Address toolbar at y=120–155, and the page below. The panel occupied roughly half the window. This confirms the stacked chrome in live CEF according to the parent's direct observation. I did not personally view that native capture; it is in the parent's tool conversation and has no saved artifact path.

Native empty-address focus, slow loading, error, hover, and narrow-width states remain unverified. Settled screenshots cannot establish animation quality or its absence. T3 comparisons below are source contracts, not claims that I ran or viewed T3.

## VQ-C01 — The browser still has multiple competing heads

**Priority:** high for the browser-quality correction. **Evidence:** directly observed in the saved headless chrome; independently corroborated by the parent's native loaded-page observation; cause traced in source.

The outer surface row already says Browser and offers the shared add-surface control. Immediately below, a large blue Browser title repeats the same identity inside a rounded card. A further row holds browser page tabs and another plus. Only the next row contains navigation and the address. On an empty browser, that nested page row remains as a whole strip with one plus and no page identity.

This has several visible costs:

- The user passes through three kinds of identification before reaching the page: the Browser surface tab, the repeated Browser heading, and the actual page tab. The generic Browser heading is more visually prominent than the specific page identity.
- The two plus controls belong to different levels, but those levels are not explained by the visual hierarchy. One adds a panel surface; one adds a browser page. Their separation is a consequence of the nested implementation.
- The inner card adds another border and edge inset beneath a panel that is already a distinct right-hand region. Other panel bodies such as Files and Changes fill the shared surface directly. Switching surfaces changes the framing language as well as the content.
- The blue Browser heading spends the same semantic color used for links on a static generic title. Source attaches no action to this heading. This is a weak affordance in a product whose user is already reporting text that looks clickable and does nothing. No native click on this title was tested, and this observation should not be conflated with the independently reported PR-link defect.
- The two tab systems use different densities and close conventions. The outer surface tab has a 16px close/icon area; the inner browser tab embeds a 28px preview-header control and additional vertical padding. This inconsistency is source-backed; the two levels are visibly different in the captures.

The parent's native observation puts the browser-only stack below the app header at about 112px. That number is useful because it measures repeated structure, not because all chrome must be minimized. The extra heading and nested tab level need to justify their cost; the present design does not make their roles clear.

**Cause:** `panel.rs:636–645,685` routes Browser through a padded legacy preview. `panel/preview.rs:451–459,498–515` applies `head_title`, `reading_head`, card fill, radius, and ring. `ferrite-pane/src/preview.rs:641–672` gives that head a 39px row and `LINK_INK`. `browser.rs:1355–1405,1490–1521` then supplies a second tab row and toolbar. Inner close controls use `head_door` at `ferrite-pane/src/preview.rs:689–707`; its 28px token differs from the unified surface strip's 16px close token.

**Existing product intent:** [ADR 0039](../../../adr/0039-the-panel-is-one-tabbed-surface-host.md) explicitly calls for one head row, identities based on what a surface shows, and removal of per-surface tab strips. The screenshot is evidence that adding the shared row did not finish integrating Browser into that model. This is stronger than a general preference for a different-looking browser.

**T3 contract:** `apps/web/src/components/RightPanelTabs.tsx:599–641,1365+` supplies page-specific browser identity within the shared surface-tab model. `components/preview/PreviewChromeRow.tsx:94–95` supplies a compact fixed navigation row. This is a structural reference, not a claim of pixel parity with a running T3.

**Correction direction:** Give each visible page one clear identity in the shared panel model, remove the redundant legacy Browser heading/card framing where the panel already supplies it, and use one deliberate tab/control density. Resolve surface ownership before restyling the remaining controls. Recoloring the current layers would leave the underlying visual problem intact.

**Acceptance:** Open one page, open a second page, switch to Files and back, and close the active page. At every state, the active page and the scope of add/close controls must be obvious. There should be no generic duplicate Browser title taking a separate row, and the browser's content and toolbar edges should follow the shared panel's layout.

## VQ-C02 — The address field has weak empty-state affordance and an expensive toolbar composition

**Priority:** medium. **Evidence:** empty address presentation directly observed in headless captures; native loaded composition corroborated by the parent; hover/focus quality source-qualified and still requires native verification.

In the saved empty browser, a static Address label is followed by a broad blank area and a forward-shaped Go control at the far edge. Nothing inside the empty input area tells the user where or what to type. The visible toolbar reads as three navigation icons, then text, then empty space, then another arrow. In the native loaded state the parent saw the same toolbar with Annotate, Address, and the URL.

The issue is not that every URL field needs a permanent outline. T3 itself uses a transparent field while idle. The issue is that Ferrite's blank field does not have a clear idle prompt or a cohesive visible address-control boundary; the source also supplies no enclosing field hover/focus treatment. The static label and separate Annotate text button consume width before the editable URL begins. That cost becomes more consequential as the panel narrows, although actual native narrow-width clipping was not tested here.

**Cause:** `browser.rs:1452–1477` inserts Annotate conditionally, then a standalone Address label, a flexible Composer, and a separate Go control. The address Composer is explicitly limited to one visible row at `browser.rs:180–184`; it is **not** an unbounded multiline input. `ferrite-pane/src/composer.rs:1084–1136` gives the input an I-beam, focus tracking, and text editing but no field background, ring, placeholder, or browser-specific enclosing treatment. Browser's wrapper adds none of those.

**T3 contract:** `PreviewChromeRow.tsx:94–95,153–215` reserves a fixed chrome row with a compact input group, URL placeholder, focus handling, and contextual open-in-browser control. `ui/input-group.tsx:8,24–25` provides hover fill and focused background/ring even though the idle field is transparent. This is evidence of a complete set of control states, not a requirement to copy a border or add controls merely for parity.

**Correction direction:** Treat the location control as one designed component with empty, idle, hovered, focused, editing, loading, and invalid states. Keep navigation grouped, make the editable region obvious, and preserve useful address width at the supported minimum panel size. The field should remain one line. Any choice between text and icon presentation for annotation must preserve discoverability and fixed geometry.

**Acceptance:** In a new empty tab, the operator can identify the editable region before clicking it. Hover and keyboard focus make its bounds and ownership clear. Entered text remains legible and controls remain reachable at narrow and ordinary panel widths. Capture these states in the installed CEF build before declaring this fixed.

## VQ-C03 — The error presentation is a raw sentence with undifferentiated recovery advice

**Priority:** medium. **Evidence:** directly observed for the headless unavailable-host state; generic source path established. Native navigation errors remain unverified.

The saved browser shows a long sentence immediately below the toolbar: the browser is unavailable in this host, followed by “Edit the address or reload to try again.” It uses ordinary body text across the width of an otherwise empty card. The cause, required next step, and generic recovery suffix have the same visual treatment.

The headless host's lack of CEF is expected. It is **not** evidence that the installed browser fails to load. The presentation still demonstrates a real design weakness: the appended recovery instruction does not match the displayed cause. Editing the address or reloading cannot make this host browser-capable. The earlier clause supplies a useful next step, but the generic suffix undermines it.

**Cause:** `browser.rs:1533–1542` combines browser-level and active-tab errors into one string and unconditionally appends the same edit/reload advice. The markup adds only horizontal/vertical padding. It does not distinguish a host capability error from a URL error or a page-load failure.

**Correction direction:** Keep errors local to the browser, identify the failed state and its relevant recovery clearly, and preserve useful context. Use the shared error/empty-state language already available in the product; no new decorative card hierarchy is needed. A native network failure needs a real retry path, an invalid address needs input correction, and unavailable embedding needs a host-related instruction.

**Acceptance:** Exercise an unavailable host, an invalid address, and a native network error separately. Each state must offer a relevant next step and must not imply that unrelated actions will resolve it. Verify text hierarchy and recovery at narrow panel width as well as the wide empty state captured here.

## VQ-C04 — A sparse side surface can become the dominant workspace region

**Priority:** high. **Evidence:** directly observed comparative headless screenshots; numeric contract supplied by the panel-runtime worker. The parent separately observed a native Browser occupying roughly half a 1920px window, but its preceding sizing history is not asserted here.

Compare [Overview first open](../evidence/panel-correction/02-overview-first-open.png) with [Overview after Files](../evidence/panel-correction/05-overview-after-files.png). The first has a compact Overview beside a visible navigation sidebar and a broad conversation. The second shows the same small amount of Overview content in a much larger region, with navigation collapsed and the conversation compressed. The [Browser after Files](../evidence/panel-correction/13-browser-after-files.png) then inherits that large region even though it has no loaded page in this host.

The runtime worker's [recorded contract](../evidence/panel-correction/overview-width-contract.json) reports Overview width 320 logical pixels before the sequence and 864 afterward, with no operator resize in the contract. At the captured 1440-logical-pixel window width, 864 is 60% of the window. The images are rendered at 2×, so screenshot pixels must not be mistaken for logical layout pixels.

The problem is proportional and behavioral: incidental use of one tool changes the amount of space assigned to later tools and the conversation. Sparse content gets the dominant region without a fresh user decision. This directly addresses the complaint that the panel opens a massive area repeatedly. An initial screenshot at the default 320px width would have missed it.

**Cause and ownership:** The panel-runtime correction owns the reproduced sequence and width-state diagnosis. Source independently shows different floors by surface in `panel.rs:144–166`: Browser/File/PR have a window-relative floor, Overview has 320px, and Files/Changes use another rule. This critique does not duplicate the sizing finding as a new feature gap.

**Correction direction:** Preserve an operator's useful allocation and distinguish remembered preference from a temporary minimum required by another surface. Recover conversation space when returning to a narrower surface unless the operator deliberately chose the wider layout. A large browser can be appropriate; forcing or retaining that allocation as a side effect is the quality defect.

**Acceptance:** Begin with Overview at a compact width. Open Files, return to Overview, close/reopen the panel, switch to Browser, then return again. Record conversation, navigation, and panel bounds at each step. Any growth must have a clear reason, and temporary tool requirements must not silently become a new lasting preference. The same sequence must be checked in the installed app.

## State changes still needing visual verification

Two existing source findings materially affect perceived finish and must remain part of this correction:

- **Browser geometry during loading:** `browser.rs:1452–1465,1522–1530` removes Annotate while loading and inserts a separate Loading row. The controls and page therefore receive different layout space. T3's `PreviewChromeRow.tsx:295–300` places loading progress absolutely at the row edge. This is source-backed support for existing BROWSER-08, not a new native-reproduced claim. Capture before/during/after a slow load and record page origin, viewport size, and address bounds.
- **Panel/sidebar motion:** existing `motion-001` and its merged right-panel finding remain valid source findings. `ferrite-ui/src/motion.rs:33–35` explicitly prescribes snapping; `cockpit/panel.rs:25–27` describes the same policy. That explains the implementation, but it does not satisfy the user's explicit request for animated opening and closing. A settled screenshot cannot measure the transition. Verify real intermediate frames, reversal, input ownership, and reduced-motion behavior when correcting it.

No native hover, focus, loading, error, or motion state is marked verified merely because a helper, token, or component exists.

## What the existing design documents do and do not settle

ADR 0039 already establishes the shared-panel integration that the Browser has only partly adopted. Older card/frame and snap policies explain why the present code looks and behaves this way. They are not evidence that the user currently finds it acceptable, and they do not override the user's direct statements that the browser is ugly, the panel is too large, and the sidebar must animate.

The appropriate next work is to complete the existing surface model and interaction contracts. A new coat of styling on the legacy Browser card would preserve the duplicated hierarchy. Adding more browser capabilities without validating the basic loaded, empty, focus, loading, error, and resize journeys would repeat the same mistake this correction is intended to expose.

This report adds a visual assessment to the existing functional findings. Its observations should be consolidated with the Browser, panel sizing, and motion findings rather than counted as an independent batch of missing features.

## Parent native follow-up

After this worker report, the parent observed a native Loading state with Annotate removed, Address shifted left and an extra Loading row reducing the page region. Closing the owned page exposed the weak empty-address state. These bounded observations corroborate the geometry and empty-presentation concerns; they do not certify focus, timing or network-error recovery. See [native corrective observations](../evidence/native-correction.md) for coordinates, installed-version limits and cleanup. Saved PNGs remain headless captures.
