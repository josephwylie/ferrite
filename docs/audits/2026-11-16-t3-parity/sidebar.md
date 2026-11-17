# Sidebar source contract

Reference commit is `4da2bc520c38cd6b36014d2888774168fbf4c77f`, confirmed with `git -C /tmp/ferrite-t3-reference rev-parse HEAD`.

## Source rules established before edits

All reference paths below are relative to `/tmp/ferrite-t3-reference`.

| Rule | T3 source | Ferrite before | Change |
| --- | --- | --- | --- |
| Default sidebar is flat and full height. It has a right border. Floating decoration only applies to `variant=floating`. | `apps/web/src/components/AppSidebarLayout.tsx:472` passes no variant. `apps/web/src/components/ui/sidebar.tsx:283` defaults to `sidebar`; `:399-420` owns default geometry. | `nav::card` has 12px exterior horizontal inset, 40px vertical inset, 18px corners, floating shadow and full outline. Column occupies 280px. | Zero exterior inset and corners. Remove floating shadow and keep right border. Column occupies 256px. |
| Default preferred width is 256px. | `apps/web/src/components/threadSidebarWidth.ts:2` | Inner card is 256px, exterior column is 280px. | Column is 256px. |
| Header includes brand followed by history arrows with 4px gap. | `apps/web/src/components/sidebar/SidebarChrome.tsx:75-82` | History arrows alone. | Add Ferrite brand before existing history arrows. |
| Search and controls have 4px gap. Controls are a gapless group of 28px buttons. | `apps/web/src/components/sidebar/SidebarThreadHeader.tsx:60-107` and `:172` | Search and each button independently separated by 4px. | Group buttons after one gap. |
| Search and footer inherit one 8px inset. | `apps/web/src/index.css:104`, `apps/web/src/components/ui/sidebar.tsx:668`, `:716` | Parent content adds 8px, search and footer add another 8px. | Remove repeated search and footer padding. |
| Card project badge, pin, project label, status follow that order. | `apps/web/src/components/sidebar/SidebarThreadRow.tsx:1459-1480` | Project badge, project label, pin, status. | Move pin before project label. |
| Interactive rows and menu buttons use pointer cursor. | `apps/web/src/components/sidebar/SidebarThreadRow.tsx:977`, `apps/web/src/components/ui/sidebar.tsx:749` | Arrow cursor. Existing unit test enforces arrow. | Set pointer and update obsolete assertion. |

The following existing rules already match and remain unchanged. T3 `SidebarThreadRow.tsx:1423-1453` sets 2px top and bottom list item padding, 78px card height with checkout, and automatic 60px height without checkout. Its content has 8px vertical and 10px horizontal padding. The first line is 20px. The next two gaps are 4px and 2px. The title is 14px, project and metadata are 12px, and card inter-item gaps are 6px. Ferrite `NAV_CARD_*`, `FS_NAV_*`, and `NAV_ROW_PAD_X` match these dimensions. T3 `index.css:1270-1272` defines dark hover, active, and selected fills at 8%, 11%, and 7%. Ferrite uses matching alpha tokens.

## Data and scope

The existing `NavState` and `ThreadRow` remain the data shape. No state machine or navigation behavior changes are needed for these layout fixes. Model the Domain keeps existing typed state intact. Laziness Protocol limits this change to drawing and sidebar geometry tokens.

The old audit directory remains untouched. Its floating-sidebar conclusions conflict with the verified source above and were not used as the specification.

## Verification ownership and limits

Parent captured the unchanged native baseline before edits at `evidence/native-before.png`. Parent owns final build, narrow tests, native screenshots, and interactions. This worker ran no tests or builds.

Proposed narrow check is `cargo nextest run -p ferrite-cockpit -E 'test(nav::tests) | test(cockpit::tests::nav_list) | test(cockpit::tests::nav_parity) | test(cockpit::tests::nav_shelves)'`. Parent should combine this with other changed-module filters in one invocation, followed by one applicable typecheck.

Ferrite keeps its existing native traffic-light and window-drag handling. Sidebar brand and history now render in the titlebar region. The sidebar topbar uses the source 52px height. The board and right panel retain their existing titlebar geometry. T3 supports user-resized sidebar width, while Ferrite retains its existing fixed-width behavior because adding resizing is outside this reported styling scope. Ferrite retains Park terminology, Factory controls, and project badges already present in its domain. No GPUI limitation prevented the corrected inset, grouping, pin order, or cursor behavior.

## Background and grain follow-up

Source verification rejects the hypothesis that T3 Code sidebar uses the root dark card surface of `#111111`. `index.css:1258` overrides the sidebar-local card to `#000000`. Its `--sidebar` resolves to that card. The later theme mapping at `index.css:1575` resolves `--sidebar` through `--app-theme-sidebar`. `packages/shared/src/themePalettes.ts:222` defines T3 Code dark sidebar as `#000000`, independently of the `#111111` card at line 187. Ferrite now paints navigation with a dedicated `NAV_BACKGROUND` token of `#000000`. Global panel paints remain unchanged. Sidebar title fades, edge fades, the edge tab, and the scoped project button share that navigation token so no old gray rectangle remains over the corrected background.

T3 `index.css:1769` supplies a 256px SVG tile with `feTurbulence` fractal noise, base frequency 0.9, four octaves, stitch enabled, and 0.035 opacity. Ferrite `nav/list.rs:420` supplies a 256px deterministic xorshift random grayscale bitmap with constant alpha rounded from 0.035. Tile size and opacity match this base T3 rule, but the noise distribution and spatial correlation differ. The T3 Chat preset separately uses a 128px tile and 0.02745 opacity at `index.css:1777`. Ferrite's retained random grain is an implementation difference. No experiment established a GPUI inability to reproduce the SVG texture, so this report makes no such claim.

## Header placement follow-up

The first built verification exposed a remaining structural mismatch. Brand and history rendered as a second row below Ferrite's 44px titlebar. T3 `SidebarChrome.tsx:62` uses `--workspace-topbar-height` for that row, and `index.css:123` defines it as 52px. T3's search belongs immediately below that topbar with the 8px group inset.

`cockpit/view_head.rs::history_arrows_element` now renders the sidebar brand and history in a 52px overlay in the existing titlebar region while navigation is open. `cockpit/nav_list.rs::nav_content` no longer inserts a duplicate header row. `NAV_HEADER_H` owns the 52px sidebar-specific boundary. The search center is therefore 52 + 8 + 16 = 76px, and history center is 26px. Existing navigation integration assertions now cover those positions, verify that the brand precedes the history arrows, and continue exercising back navigation and sidebar folding. The parent owns execution of those assertions and the next built screenshot.

This local header correction leaves global board and panel heights untouched. No framework limitation is claimed for that remaining difference. Add `test(cockpit::tests::navigation)` to the parent's narrow test filter for this final change.


## Resize and grain completion

This follow-up supersedes the earlier fixed-width scope statement and retained xorshift grain statement. The pinned T3 revision is unchanged.

`threadSidebarWidth.ts:1-38` defines a 256px default. The minimum is the larger of 208px and the rounded-up measured brand width. The maximum is the larger of that minimum and the rounded-down viewport width minus 640px. `AppSidebarLayout.tsx:432-456` resolves these bounds on viewport changes. Its `resetSidebarWidth` removes the stored preference, and `:491` binds that reset to a rail double-click. `ui/sidebar.tsx:473-543` keeps a drag temporary until release and cancels it when the sidebar collapses.

Ferrite now stores `Settings.nav_width_px` as an optional preferred width. `None` means the 256px default and is omitted from saved JSON. `cockpit/columns.rs` owns the temporary drag, finite-value fallback, brand measurement, width limits, release persistence, and reset. Brand measurement uses the actual shaped Ferrite text plus the titlebar inset, history controls, gap, right padding, and border. A viewport resize clamps the displayed width without overwriting the preference. Collapse cancels an unfinished resize.

The resolved width feeds the column, card, content, cached navigation view, row text, slim row text, dragged row, hover detail anchor, header, board, panel room, automatic folding, and edge tab. The existing collapse animation still interpolates the expanded width. The resize rail uses the source 16px hit area across the column boundary. Double-click clears the preference without collapsing the sidebar.

`SidebarThreadRow.tsx:977-993` gives the active row precedence over multi-selection, followed by hover. Multi-selected rows retain the selected fill on hover. `Sidebar.logic.ts:897-921` excludes active and selected rows from receding. Ferrite now follows both rules for card selection, including working rows. Existing 78px and 60px card heights, 8px vertical padding, 10px horizontal padding, 20px first line, 4px and 2px line gaps, 6px item gaps, and 14px/12px typography match the source.

`index.css:171-174` puts the Apple system font before Inter in the default UI stack. Ferrite's existing `.SystemUIFont` resolves that macOS default. No Inter override was added.

The exact SVG noise source from `index.css:1769` is bundled as `crates/ferrite-cockpit/assets/t3-surface-grain.svg`. Its adjacent license preserves the T3 Tools MIT notice. GPUI's existing SVG renderer supports `feTurbulence`, so the sidebar now caches a raster at the current display scale and tiles it behind content. Header and body share the same vertical tile origin. The header uses the same grain. The source remains a 256px tile with fractal noise, frequency 0.9, four octaves, stitching, and 0.035 opacity. The prior xorshift bitmap is deleted. Native raster output remains subject to the parent's screenshot comparison.

Model the Domain placed the optional preference and transient drag in their existing settings and column owners. Laziness Protocol kept the change local to sidebar geometry and reused GPUI's existing SVG renderer.

The worker added regression coverage for resize geometry, release persistence, viewport clamping, collapse restoration, double-click reset, abandoned drag cancellation, finite-width fallback, brand limits, and selected working-row opacity. No builds, tests, GUI runs, or worktrees were created by this worker. Parent verification remains outstanding.

The parent's narrow test filter needs `test(nav::tests) | test(cockpit::columns::tests) | test(cockpit::tests::nav_list) | test(cockpit::tests::nav_parity) | test(cockpit::tests::nav_shelves) | test(cockpit::tests::column_motion) | test(cockpit::tests::navigation)` for `ferrite-cockpit`, plus `test(settings::tests)` for `ferrite-core`. Combine these with the other workers' changed-module filters in the parent's serial run. Follow with the one parent typecheck and the native E2E pass.


## Isolated verification corrections

The parent run in `/tmp/ferrite-parity-isolated-ui-tests.log` exposed two sidebar failures. The resize regression reached 320px and widened title text correctly, but release on the rail left the stored width unset. GPUI `elements/div.rs:3060-3103` dispatches click listeners during the mouse-up bubble phase. The rail stops that propagation, so the root's bubble listener never committed the resize. Sidebar release now commits in the root capture phase before any child consumes the event. The test still releases on the actual rail and checks both in-memory and disk state.

The collapse-level fixture used a 550px window. Sidebar clamping now leaves at least 300px for the cell there, so Transcript is the correct expanded level. The fixture now uses 480px. Even at the 208px sidebar minimum, the expanded cell is at most 272px; folding restores 480px. These bounds straddle `docview.rs`'s 300px Transcript threshold without changing production zoom rules.

Fix Root Causes moved resize completion to the event phase that can receive every release. The existing red tests remain the regression checks. This worker made these corrections only in the `ferrite-t3-parity` worktree and ran no Cargo commands, builds, or GUI checks. Parent rerun remains pending.


## Resize event routing correction

The parent's second run in `/tmp/ferrite-parity-ui-corrections.log` proved that changing only the root event phase did not fix release persistence. The preceding capture-phase conclusion was incomplete.

GPUI `Interactivity::capture_any_mouse_up` in `elements/div.rs:235` checks `hitbox.is_hovered` even during capture. The resize rail uses `occlude`. `window.rs:1187` stops hit testing at that blocking hitbox, so the root is not hovered and its capture listener is skipped. The same condition gates the root move handler. An initial jump can resize while the pointer is outside the old rail, but small moves after the rail repaints can stop reaching the root.

The rail now installs window capture listeners during canvas painting, following the existing `image_viewer.rs::follow_press` pattern. These listeners check the owned resize state directly and receive move/release events regardless of the hitbox under the pointer. They consume events only while the sidebar owns a resize. The old root sidebar handlers are removed. The regression test now makes a second 4px move inside the repositioned rail before release, so it checks both continued movement and persistence. No other input owner changed.

This correction was made only in the isolated worktree. Parent Cargo and native verification remain pending. No worker builds or GUI runs were performed.


## Sidebar header correction after native build 4

The native build 4 screenshot confirms that the sidebar header already uses the 52px `NAV_HEADER_H`. Its history center is at 26px and search center at 76px. The apparent 44px bottom edge came from `plane_seams`, drawn afterward across the whole window. That horizontal rule now starts at the sidebar's animated width. The board and panel still use their existing 44px header, and native traffic-light geometry is unchanged.

Pinned `AppSidebarLayout.tsx:385-405` owns a persistent 28px sidebar toggle at `workspace-controls-left + 1px`, centered in the 52px topbar. macOS desktop uses a 90px controls inset outside fullscreen. The ordinary inset is 12px. `ui/sidebar.tsx:246` places the brand after the inset, 28px control, and 12px gap. Ferrite now follows these positions, including the 130px brand inset on ordinary macOS windows. The existing width measurement includes this complete header footprint without polling layout.

The former middle rail collapse handle is removed. A header toggle with open and closed panel glyphs remains visible when the sidebar collapses. Brand and history collapse with the sidebar, matching their ownership in `SidebarChrome.tsx`; the board title clears the remaining control. History uses the source 14px arrow glyphs rather than the former chevrons. Ferrite branding remains unchanged.

Regression assertions now exercise header-toggle collapse and reopening, fixed toggle bounds, brand/history ordering, hidden collapsed history, board-title clearance, and the divider's sidebar exclusion. Existing automatic-collapse and provider-navigation tests target the header control. The prior midrail-specific unit assertion is replaced with a titlebar-control geometry assertion.

No Cargo, native UI, or screenshot runs were performed by this worker. The parent should run the changed `nav::tests`, `cockpit::tests::nav_parity`, `cockpit::tests::navigation`, `cockpit::tests::nav_list`, `cockpit::tests::provider_navigation`, and `cockpit::tests::thread_groups` tests, together with `ferrite-ui`'s `icons::tests`, before the native comparison.


## Column animation endpoints after the header correction

The parent run in `/tmp/ferrite-parity-final-ui-tests-retry.log` reached a 262px settled sidebar in the GPUI test font. Two column-motion tests still expected the unconditional 256px default. The production minimum matches the pinned `SidebarBrandWidthProbe` and `threadSidebarWidth.ts`. It includes the 130px brand inset, measured brand, 4px gap, two 28px history controls, 12px right padding, and 1px border, then rounds up and takes the larger of that value and 208px. A measured header can therefore raise the 256px preferred default.

The animation tests now derive their endpoint from the rendered `nav-head` bounds, right padding, border, preferred width, and viewport limit. They retain the same assertions for intermediate motion, unsquashed column face, reversal continuity, complete closure, reduced-motion snapping, and zero animation frames at rest. The resize-persistence regression uses the same measured bounds for its default, drag, narrow-viewport, and reset expectations. No expected value was replaced with a hardcoded 262px value.

Production sizing and rendering code are unchanged by this correction. No Cargo or native UI runs were performed by this worker. Parent rerun remains pending.
