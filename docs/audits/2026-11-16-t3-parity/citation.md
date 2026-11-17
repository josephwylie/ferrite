# Citation comment popover

Reference source is `/tmp/ferrite-t3-reference` at `4da2bc520c38cd6b36014d2888774168fbf4c77f`. The reported before state is preserved as `evidence/citation-user-before.png`.

## Source contract

`apps/web/src/components/chat/AssistantCitationChip.tsx:237-252` anchors the comment popup to the last client rectangle of the selected source range, clamped to the source message width. At `:286-300`, the pencil is the comment trigger and the popup opens below a source anchor, or above the chip when no source anchor is available. At `:301-309`, its textarea receives focus and the caret moves to the comment end. At `:325-328`, it uses medium width and compact padding.

`apps/web/src/components/ui/popover.tsx:16-27` maps medium width to 320px, compact padding to 12px inline and 8px vertical. Its default side offset is 4px. `AssistantCitationCommentEditor.tsx:81-107` uses a 64px minimum and 160px maximum textarea, optional-comment placeholder, 14px desktop text, Enter to save and Shift+Enter for a line break. At `:115-132`, Cancel precedes Save and both use the extra-small button recipe. `ui/button.tsx` defines that recipe at 28px high. `assistantCitationCommentDismissal.ts` commits a changed draft on outside dismissal and discards it on Escape.

## Ferrite cause and change

The existing `Citations` model already owns the open reply and optional editing field. Its attachment tray renderer appended `panel()` as an ordinary full-width child inside `pending-attachment-body`, which scrolls. Thus the card expanded the composer tray rather than floating near its subject. The card also used transparent `form_button` controls without local geometry. This directly explains the screenshot's broad block of plain words.

The same state now records an optional source selection rectangle and the measured chip trigger. The transcript captures the selected range's last painted rectangle before clearing the selection and passes it with the existing Reply event. The composer opens the comment field directly after Cite. The popup is a deferred anchored overlay at 320px, with a 4px source gap and 16px viewport collision margin. The chip pencil directly reopens the editor using its measured trigger as the fallback anchor. The textarea focuses once on layout. Editing shows the field and properly sized Cancel/Save controls, without the old quote/source/action text stack. Save and Cancel close the popup. Outside presses save a changed comment and close it without stealing the newly selected focus.

The chip's existing quote/source/remove inspection remains available as a bounded floating card. Those controls receive an explicit local 28px button recipe. No global form-button styling changes.

## Verification

The `cite-comment` visual-reference scene renders the comment editor using a synthetic source rectangle. Parent owns final screenshot inspection and comparison. `cockpit::tests::citing` exercises pointer selection and Cite, automatic textarea opening, 320px popup width, 64px textarea minimum, 28px controls, source placement, chip fallback, save, cancel, multiline comments, outside dismissal, source navigation and draft persistence. Parent owns final typecheck and broader changed-module checks.

## Remaining differences

Ferrite stores a native painted selection rectangle, while T3 retains a live DOM Range and observes source availability. The native initial popup is anchored to the captured source geometry. Reopening uses the chip. No live native range-following or voice controls were added. No demonstrated GPUI limitation is claimed. T3's exact voice workflow and maximum-comment validation are outside this reported popup defect. The legacy source/quote inspection remains on the chip to preserve existing behavior.

The source-navigation regression exposed an invalidation dependency. The source action installed citation marks and `settle_cited` rendered once with an empty paint report. Its existing bounded reveal loop requested animation frames, but a cached transcript reused its output on those frames and did not advance the loop. The old in-flow card had incidentally caused more transcript layout. The loop now defers a notification of its view after the current update only while its existing bounded wait remains. An `on_next_frame` notification did not advance the cached view in the isolated regression, whereas the deferred notification did. The source scroll algorithm remains unchanged. There is no broad window refresh or row-top fallback. This identifies the observed cache interaction rather than claiming all GPUI views have the same limitation.

Fix Root Causes changed placement ownership before button details. Model the Domain extended the existing citation state with its anchor rather than creating another popup owner. Laziness Protocol kept the change local and preserved the existing reply data and persistence.

## Performance review

The popup is constructed only for the single open citation. Its comment field remains the existing nested Composer field. The new source rectangle is a small copy in the transient event and citation state. The chip bounds callback refreshes windows only when those bounds change. Focus is requested once per new field, not on every render. There is no polling, background task, or additional transcript traversal in the popup path. These are code observations, not a measured performance claim.

Final citation interaction verification ran `cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::citing::)'`. All 17 tests passed. Nextest run ID was `dacc846f-dcb0-409a-af2b-fb787d7b33a3`. Parent owns final visual refinement, screenshots and typecheck after the full scoped diff.

## Parent native comparison

`evidence/t3-citation-native.png` captures selecting assistant text and pressing Cite in installed T3. It confirms automatic focus, source anchoring and the compact editor. Cancel removed the temporary empty citation. No message was submitted. The source palette defines the completing action as `#346bf1` with white text. Ferrite now reuses its existing `SEND_LIT` and `ON_SEND_LIT` tokens locally for Save. The final parent UI check passed after this styling change.
