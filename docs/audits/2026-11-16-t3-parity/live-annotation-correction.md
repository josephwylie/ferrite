# Browser annotation layout correction

The goal is to align Ferrite's existing browser annotation controls with T3's hierarchy and placement. This report covers the retained capture editor. The live capture overlay belongs to the cockpit and is verified separately.

## Reference evidence

T3 source is pinned at `4da2bc520c38cd6b36014d2888774168fbf4c77f` in `/tmp/ferrite-t3-reference`.

The live T3 captures use the local Work orders fixture at `http://127.0.0.1:60270/`. These files record actual application states.

- [Selected heading with compact composer](evidence/live-browser/t3-annotation-element.png).
- [Expanded heading editor](evidence/live-browser/t3-annotation-expanded.png).
- [Freehand stroke and compact composer](evidence/live-browser/t3-annotation-draw-stroke.png).
- [Readable drawing detail](evidence/live-browser/t3-draw-detail.png).
- [Ferrite before the correction](evidence/live-browser/ferrite-editor-before.png).
- [Ferrite after the correction](evidence/live-browser/ferrite-editor-after.png).
- [Readable Ferrite detail](evidence/live-browser/ferrite-editor-detail.png).

The Ferrite baseline confirms that the old editor placed the title, URL, tool controls, ink controls, history, comment field, and actions in one vertical flow. The retained screenshot occupied the remaining space. T3 keeps the page primary and overlays its toolbar and composer.

## Source rules

| Rule | Pinned source | Ferrite implementation |
| --- | --- | --- |
| Overlay does not consume page height. | `apps/desktop/src/preview/PickPreload.ts:561-567`, `594-607`. | Full-size retained stage. Floating controls are laid out during prepaint after the image. |
| Toolbar is centered at 10px from the page top. It has 4px padding, 2px gaps, a border, and an 8px radius. | `PickPreload.ts:594-600`. Radius resolves through `apps/web/src/index.css:1115` and `apps/web/src/browser/annotationTheme.ts:11`. | Top-center overlay, same dimensions and border grouping. |
| Main tools are 32px tall with 10px horizontal padding and 14px text. | `PickPreload.ts:1233-1253`. | Existing seven drawing tool buttons use those measurements. |
| Secondary controls are 28px tall with 8px horizontal padding, 12px text, and 6px radius. | `PickPreload.ts:495-510`. | Existing ink, width, history, and cancel controls use these measurements. |
| Selected tools use primary text and 10% primary fill. | `PickPreload.ts:934-944`. | Local annotation controls use `SEND_LIT`, `#346bf1`, with alpha 26 of 255. |
| Composer width is the smaller of 360px and page width minus 16px. It has a border and 12px radius. | `PickPreload.ts:602-607`. | Composer measures against the current stage, capped at 360px and inset by at least 8px. |
| Composer row has 8px padding and 8px gaps. Expand control is 32px square. Comment has a 32px minimum, 96px maximum, and 14px text with 20px line height. | `PickPreload.ts:609-630`. | Same row dimensions. Existing Composer keeps four visible text rows. Empty field uses the source placeholder. |
| Attach action is 32px tall with 12px horizontal padding and primary fill. | `PickPreload.ts:855-864`. | Existing Added event is emitted by the compact Attach button. |
| Compact composer tries right, left, below, then above the union of all targets. It chooses the least overflow and clamps to 8px edges. | `PickPreload.ts:1255-1335`. | Pure `editor_position` applies that candidate order and clamp within the retained stage. `feedback_bounds` unions the recorded selection and existing drawings. |
| Adjust expands secondary content without shrinking the underlying page. | `PickPreload.ts:1340-1363`. | One `options_expanded` boolean reveals existing ink, width, history, cancel, and text label controls beneath the composer. Selecting Text reveals its field. |
| Selection outline has 2px stroke, 3px radius, and 10% primary fill. | `PickPreload.ts:345-356`, `55-56`. | Capture review uses 2 CSS pixel stroke and 10% fill. Its existing path painter still draws square corners. |

The default dark popover is defined by `apps/web/src/index.css:1187-1203`. Annotation chrome uses Ferrite's existing `CARD` dark fill instead of the previous lighter `FLOAT` fill. This choice is local to this editor.

## State and ownership

The editor still owns retained feedback, not a live browser document. Capture bytes, capture geometry, seven drawing variants, history, gesture capture, comment text, cancellation, and the Added event retain their existing ownership. The only added persistent state is whether secondary controls are expanded. Layout uses the current prepaint bounds without a geometry polling task or stored screen coordinates.

The floating groups occlude the stage's pointer hitbox. GPUI's ordinary hitboxes permit interactions with elements behind them. Without explicit `occlude`, the stage consumed clicks over the compact composer. The first narrow test run caught this in the real expand and Attach interactions. The correction uses GPUI's existing blocking hitbox behavior on the toolbar and composer.

The Model the Domain principle kept layout separate from immutable evidence. The Laziness Protocol principle kept the existing capture model and interaction handlers instead of introducing another annotation mode or a new drawing model.

## Explicit differences

T3's live toolbar contains Select, Region, Draw, and Erase. Ferrite's retained editor already exposes Pen, Line, Arrow, Rectangle, Ellipse, Steps, and Text. The correction preserves those tools and their discoverability. Its toolbar is wider than T3's toolbar. The cockpit owns Select and Region before capture. The retained editor does not show controls that imply live DOM selection or erasing selected DOM nodes.

T3's expanded panel mutates selected DOM styles. Ferrite's retained capture model has no live DOM style editing capability. Its expanded panel contains the existing drawing options. This is a domain boundary, not a GPUI limitation.

T3 uses `backdrop-blur-xl` behind translucent chrome. The vendored GPUI `Style` implementation exposes shadow blur but no backdrop filter. This editor uses opaque dark chrome and the existing float shadow. It does not blur the captured page behind the controls.

Ferrite uses its existing system UI font. T3 forwards its configured sans font through `annotationTheme.ts:25`. Font sizes and line heights are matched here. Font face parity is not claimed.

Step and text drawings contribute their insertion point to the composer anchor union. T3's reference only needs selected DOM rectangles, regions, and freehand stroke bounds. Existing step and text glyph extents are not measured into that union.

## Verification status

The worker formatted only the owned editor files and ran `git diff --check` for those files. It did not run a build or tests. The parent owns serial verification and native screenshots. The first parent narrow run passed geometry assertions but failed expand and Attach input. After the overlay hitbox correction, the parent reported six narrow tests green in nextest run `adbab413-6cc5-4b0a-97f1-1612aa296ea8`. These include all three pane editor tests, two retained capture cockpit tests, and machine capture protection. The parent is running the final typecheck and native bundle verification.

The narrow tests cover the real seven-tool pointer flow, ink and width changes, text and comment input, Add and Cancel events, and immutable capture preservation. Additional assertions cover the full capture canvas, collapsed 360px composer, 304px composer at 320px stage width, 32px expand action, candidate placement order, and edge clamping.

The parent captured the built native after state. The worker independently viewed the full capture and readable detail against T3's selected heading and drawing captures. The after state shows the full capture canvas, floating top toolbar, primary selected state, tinted selected heading, and compact anchored composer. The existing seven tools still make Ferrite's toolbar wider. No material collapsed-editor layout error was found in that comparison.

The source dimensions and narrow geometry assertions support the 360px compact composer, 32px actions, and 8px placement rules. The screenshots establish that those controls now overlay the page instead of shrinking it. No numeric visual parity percentage is claimed. The parent's native drawing, expansion, and Attach exercise remains pending.


## Completed native exercise

The parent inspected the real native drawing, expanded options, typed comment, attached result, and retained review. The screenshot links below show these states. The saved annotation preserves the selected h1, exactly one blue pen stroke, comment text, and the original image hash.

- [Drawing and relocated composer](evidence/live-browser/ferrite-annotation-draw-after-1x.png).
- [Expanded existing options](evidence/live-browser/ferrite-annotation-expanded-after-1x.png).
- [Clean final comment](evidence/live-browser/ferrite-final-comment-1x.png).
- [Attached to conversation](evidence/live-browser/ferrite-annotation-attached-after-1x.png).
- [Side-by-side actual pages](evidence/live-browser/paired-annotation-after.png).

The clean native result and the separate provider routing incident are recorded in [native-e2e-result.json](evidence/live-browser/native-e2e-result.json). Native redo is not claimed because that attempted click used a stale coordinate after undo moved the composer. The narrow pointer test covers redo. No numeric similarity score or exact parity claim is made.
