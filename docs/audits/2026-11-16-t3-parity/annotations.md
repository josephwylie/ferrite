# Browser annotation controls

Reference commit is `4da2bc520c38cd6b36014d2888774168fbf4c77f` in `/tmp/ferrite-t3-reference`. These rules come from the pinned source. The parent also captured installed T3 in `evidence/t3-native.png`. That screenshot is supplementary because its binary commit was not established.

## Rules established before editing

The owner is the desktop preview guest preload, not the React PreviewView. `apps/desktop/src/preview/PickPreload.ts:495-508` creates annotation buttons at 28px high with 8px inline padding, 6px corner radius, 12px medium sans text, transparent border and accent hover. `:1243-1253` overrides drawing tools to 32px high, 10px inline padding and 14px text. T3 itself uses text labels for these tools.

The toolbar at `:594-600` uses a bordered popover, 4px padding and 2px gaps. `:934-944` refreshes selected tools with primary ink and a 10% primary fill. The editor at `:602-607` is a separate bordered popover capped at 360px wide, inset 8px from the viewport. `:624-630` gives the comment 32px minimum height, 96px maximum height and 14px/20px text. `:855-864` makes Attach a 32px primary action with 12px horizontal padding. `:503-508` provides explicit framed style inputs. `Annotation.css:1-18` maps these utility classes to the guest's theme variables. The theme is installed into its closed shadow root by `PickPreload.ts:565-580`.

Ferrite does not have a missing CSS injection problem. Its native retained-image editor calls `ferrite_ui::components::form_button`, whose documented resting ground is transparent, and supplies no local control dimensions or grouping surfaces. Drawing tools and widths changed text color for selection, but had no selected fill. The label and comment wrap bare Composer fields without input framing. The earlier live selection toolbar has the same missing geometry. This is the shared root of the raw-text appearance. No global button primitive was changed because other Ferrite surfaces intentionally use its transparent recipe.

## Changes

The existing Tool enum and Ink remain the data shape. Local control, control-group and field recipes gather the missing visual rules. Drawing tools use 32px height and 10px horizontal padding. Style options use 28px height and 8px padding. Both use 6px corners. Groups use 4px padding, 2px gaps and a visible border. Selected tools and widths receive 10% blue fill plus blue ink. The live selection toolbar uses the same dimensions and selected treatment. Text fields have an explicit background and border. The completing action is 32px high.

`annotation-editor` is a new visual-reference fixture using retained immutable evidence. The fixture hook is compiled only with the visual-reference feature. Existing pointer-feedback tests now assert the 32px drawing-tool height and 28px style-option height, in addition to exercising tools, ink, width, undo, redo, typed labels, comments, add and cancel.

## Verification and limits

The parent owns builds, runtime captures and test execution. Suggested narrow command is `cargo nextest run -p ferrite-pane -E 'test(annotation_editor_)'`. Cockpit browser annotation tests cover the live selection toolbar.

The initial implementation incorrectly treated the editor's full-panel placement as outside scope. The live comparison on 2026-11-16 disproved that acceptance decision. T3 keeps the page as the canvas and positions its compact composer beside the selected geometry. Ferrite's full form and reduced screenshot were material structural differences. The follow-up correction and real native evidence are recorded in [live annotation correction](live-annotation-correction.md).

Ferrite's immutable retained capture and existing Pen/Line/Arrow/Rectangle/Ellipse/Steps/Text tools remain its domain. T3's live DOM mutation and Select/Region/Draw/Erase capabilities are not added. These capability differences do not justify leaving the shared canvas, hierarchy, density or composer placement different.

Fix Root Causes shaped the shared local control recipes instead of separate button tweaks. Model the Domain kept the existing Tool enum and Ink rather than introducing presentation state that could diverge from the actual selected values.

The parent visual check caught an additional occlusion bug. The empty-browser notice covered the retained editor when its live page was absent. `browser.rs` now suppresses that notice while an annotation session owns the body. The retained-editor regression asserts that the notice is absent. The parent also placed tool and option typography on their content nodes because the toolkit button inserts its own label text-size wrapper.

The completing action also reuses Ferrite's existing T3 blue `SEND_LIT` token and white `ON_SEND_LIT`. The global neutral primary button recipe is unchanged.
