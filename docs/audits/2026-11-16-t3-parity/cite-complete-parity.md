# Cite popup parity

Reference checkout `/tmp/ferrite-t3-reference` is pinned to `4da2bc520c38cd6b36014d2888774168fbf4c77f`. This report supplements the earlier audit. It does not replace its evidence.

## Source contract

- `apps/web/src/components/chat/AssistantCitationChip.tsx`, lines 178–375, separates the citation source link from the pencil trigger. The source link navigates. The pencil opens the editor. The editor opens below a selected source and above a reopened pencil, aligned to the trailing edge with a four-pixel gap. Its source rectangle is the final range rectangle, clamped horizontally to the source message. Source observation and dismissal occupy lines 217–258.
- `apps/web/src/components/chat/AssistantCitationCommentEditor.tsx`, lines 63–153, validates the raw JavaScript string length, saves with Enter, saves and sends with Command/Ctrl+Enter, adds a newline with Shift+Enter, and cancels with Escape. Overlong text displays the 8,000-character message and a disabled “Shorten comment” button. The textarea has a 64-pixel minimum and 160-pixel maximum height.
- `apps/web/src/components/chat/assistantCitationCommentDismissal.ts` closes an untouched or trim-equivalent draft, commits a changed valid draft on ordinary dismissal, and keeps a changed invalid draft open. Escape does not commit.
- `apps/web/src/components/chat/AssistantCitationSource.tsx`, lines 23–93, retains a separate source highlight, repairs source ranges after mutations, and disposes observation when the source disappears.
- `apps/web/src/lib/assistantTextSelection.ts`, lines 39–120, normalizes whitespace, keeps 32 UTF-16 units of context without splitting a surrogate pair, resolves contextual matches, and accepts a unique quote when its context changes. Ambiguous contextual matches fail rather than choosing the first occurrence.
- `apps/web/src/components/ComposerPromptEditorTiptap.tsx`, lines 477–549, removes a newly inserted citation on Cancel. Cancel on a reopened editor discards the comment change and keeps the existing citation.
- `packages/shared/src/assistantCitations.ts`, lines 106–116, uses the comment as the chip label when present. Otherwise it uses the quote. Labels truncate at 64 UTF-16 units.
- `apps/web/src/components/ui/popover.tsx`, lines 17–38 and 82–151, supplies the 320-pixel popup, compact padding, and viewport collision handling.

Voice controls are excluded by the user.

## Implementation

`Citations` remains the sole owner of citation editor state. The previous inspection panel and its independent open state were removed. A citation body navigates to its source. Its pencil opens the optional comment editor. Raw UTF-16 length gates Save, keyboard submission, outside dismissal, and the main composer send path. A newly inserted citation is removed on cancellation. A reopened citation preserves its saved comment on cancellation.

The popup uses the existing native `Positioner` for side flipping, trailing alignment, viewport clamping, and the four-pixel gap. It measures the pencil itself when reopened. It uses a 320-pixel width, 12-pixel horizontal padding, eight-pixel vertical padding, a 64–160-pixel field, and 28-pixel buttons.

Native `TextSelectionAnchor` retains the existing selection document's exact logical ranges before ordinary selection is cleared. The native renderer paints the retained highlight independently of ordinary selection and Find. It reports the actual final line rectangle after wrapping and scrolling. Atomic inline cards report their own rectangles. Existing window selection frame generations determine source availability, including reused cached subtrees. No timer or polling loop was added.

A source mutation is repaired with normalized quote context. A disappearing source commits a valid changed comment and closes the popup, restoring focus when the editor held it. An invalid comment remains open above its pencil. Dropping the editor releases its retained range.

`Reply` now has an optional, backward-compatible serialized `Selector` containing the native text member ordinal, normalized UTF-16 offsets, and prefix/suffix context. Reopening a citation restores its source highlight without moving the popup away from the pencil. Legacy drafts without a selector use a unique-text match. Ambiguous matches do not highlight an arbitrary occurrence.

## Verification ownership

This delegate did not build, run tests, launch a GUI, or create a worktree. The parent owns serial verification because of disk pressure. `rustfmt` parsed the new anchor helper, citation editor, and citation tests. `git diff --check` passed for the citation and native selection changes before handoff.

The added or updated tests cover UTF-16 limits, cancellation, source navigation without inspection, save versus send, outside dismissal, a moving source, source streaming, valid and invalid source disappearance, pencil toggling, repeated passages, selector serialization, and reopening a restored draft. Native helper tests cover contextual repair, ambiguous and overlapping duplicates, whitespace normalization, multiline ranges, and UTF-8 boundary preservation. Relevant test scopes are `replies::tests`, `composer::citations::tests`, `cockpit::tests::citing`, and `text_selection::anchor::tests`.

Runtime and screenshot parity remain unverified until the parent's native verification completes. There is no screenshot evidence in this delegate report. The synthetic visual-reference scene now opens above its chip because it has no real native selected range; native source placement is exercised through actual selection in the citation tests.

## Limits that need explicit interpretation

A durable selector describes one native text member, which covers the normal assistant message and its multiple paragraphs. A Ferrite selection spanning independent native members can be retained while its initial popup is open, but it has no single durable selector. This is Ferrite's broader cross-message selection behavior, beyond T3's source-scoped citation selection. Reopened legacy or cross-member quotations are highlighted only when reconstruction is unambiguous.

Native font shaping, focus dispatch, popup rendering, and source lifecycle tests must pass before calling the result verified. The implementation reuses native source projections and the existing popup positioner; it does not substitute a fixed screenshot coordinate or Find query for a selected range.

## Native focus follow-up

The parent's native run found physical typing entering the main prompt while the citation popup remained open. The comment's initial focus now participates in the existing root focus policy before prepaint; the standalone fallback runs after drawing, when GPUI can schedule the focus redraw. The whole padded textarea now focuses its child editor on press. A new regression, `citation_comment_takes_focus_on_open_and_across_the_whole_textarea`, checks initial typing and clicks on the placeholder and lower padding after focus has moved to the main prompt. This delegate ran formatting and diff checks only; the parent must run that regression and repeat the native interaction before considering this symptom resolved. The parent also observed unrelated native lightbox/address input failures, so these local changes are not claimed to explain the broader native issue.

The subsequent native focus trace identified the shared cause: a persisted `File(image)` surface in an inactive Thread called `Preview::open_image` on every render. Images intentionally open in a transient lightbox without changing `Preview::document`, so `sync_previews`' document-path check never converged; it repeatedly focused that inactive viewer and reopened it after closing. The native trace showed main/address focus changing to the same image-viewer handle before the root restored the main composer. `sync_previews` now forgets this stale surface only when there is no retained document. Explicit image opens and retained annotation documents keep their existing behavior. The regression `a_restored_image_tab_cannot_steal_another_threads_comment_or_address` covers persisted inactive image state, Cite and browser-address typing, and deliberate lightbox opening/closing. Parent-owned tests and native re-verification remain required after this fix.
