# Quote, citation and attachment audit

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants quoting and attachments to be production quality without having to enumerate every broken interaction. This pass investigates the reported ugly attachment and quote presentation, the reported Cite regression, and the inability to attach personal text to a citation. It follows creation, editing, sending, display, retry, history and restoration against current T3 source.

## Overview

The missing per-citation comment editor is source-proven. The current implementation can render comments supplied by a test or serialized prompt, but does not let the user author them through Cite. The earlier composer report overstates this behavior. Ordinary main-composer text is a separate path and remains supported in source; whether its focus works after an actual click belongs to the installed-app check.

There are ten source-proven findings, one inferred overflow risk and one unresolved runtime regression in this supplement. Two source-proven findings are P1. The unverified Cite regression also has P1 triage priority because the user reports a broken primary interaction, not because this worker reproduced it. No P0 finding is asserted.

The visual criticism has specific support. A prose citation occupies an 88 pixel square, hides most of its text and offers no disclosure. An attachment may falsely say Failed even when the rendering library supports its format. These are actionable readability and state defects. This report does not claim measured contrast failure, a screenshot-proven clipping defect or complete visual parity. A square crop alone is not a Ferrite defect; T3 also crops compact thumbnails.

## Method and limits

- Ferrite baseline and inspected HEAD are `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- T3 baseline is `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, inspected at `/tmp/ferrite-t3-audit-source-20261113`. All T3 links below pin that commit.
- Read the audit brief, composer and transcript reports, the Codex pstack adapter, the Investigation playbook, How and Unslop instructions. The parent owns the workflow and runtime. No agents were spawned.
- Throughput checkpoint is n/a, read-only investigation.
- Used repository-wide caller searches and numbered source reads. No app code, tests, settings, dependencies, user data or Git branches were changed. Only this report and its issue JSON were written.
- No tests or app were run. Existing tests were inspected to distinguish injected data from reachable interactions. Their existence is not a green result or installed-build proof.

## Key concepts

Ferrite `Reply` has only `quote` and `text`. It has no source thread, message ID or selected range. The composer holds replies and file paths outside its main editable line. Quote blocks lead the serialized prompt; file tokens trail it. Drafts, queue, retry and history reuse that string. Provider adapters rediscover file and skill input by scanning it. See `crates/ferrite-pane/src/quote.rs:50-118`, `crates/ferrite-pane/src/composer.rs:494-554` and `crates/ferrite-core/src/prompt_files.rs:15-93`.

T3 citations carry environment, thread, message, text, optional comment, offsets and surrounding text. The editor stores a citation node, exposes a comment popover, and serializes a versioned reference. Provider expansion identifies quoted material and user comments separately. File/image records carry separate attachment identities and upload state. See [citation schema](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/assistantCitations.ts#L11), [citation node](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L443) and [provider expansion](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L148).

## Traced flows

| Stage | Ferrite result | Current T3 comparison |
| --- | --- | --- |
| Select and cite | Mouse-up captures text after selection settles. Focus and quote gates apply. Mouse-down emits an empty-comment Reply. `transcript/reply.rs:59-95,206-220`. | Selection observer keeps source identity and offset context. A semantic button supports Tab. [Selection toolbar](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L124). |
| Attach whole response | Context menu passes full Markdown through the selection length limit and silently ignores rejection. `transcript/message_menu.rs:103-123,168-171`. | Whole-response action constructs a source-aware response context. [Assistant rows](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineAssistantRows.tsx#L220). |
| Add own words | General editor text remains separate. No quote-comment UI exists. Cards expose only remove. `composer.rs:638-654`; `attachments.rs:551-618`. | Cite inserts at the cursor and opens a comment editor. Save, Cancel, multiline and save-and-send exist. [Insertion](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatComposer.tsx#L2989), [editor](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L81). |
| Attach file or screenshot | Paths are deduplicated, not copied. Clipboard images spill to a temporary file. The preview reads the local file. `composer.rs:572-619,2104-2124`; `preview.rs:271-290`. | Intake creates ID, file data and metadata, prepares images and reports failures. [Intake](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L204). |
| Send | Compose quotes, review text, annotations and file tokens, then take and clear. Provider adapters scan the resulting string. `composer.rs:516-554`; `cockpit.rs:5552-5596`; provider wire citations in finding 007. | Snapshot and prepare attachments, then dispatch text/context and attachments separately. [Send snapshot](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L769), [dispatch](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1292). |
| Sent presentation | Quote rows clamp to two lines. Images are raw clickable previews. File/quote decode order differs from draft restore. `attachments.rs:505-543`; `pane.rs:3226-3241,3543-3557,3800-3838`. | Citation card opens source; its comment is full whitespace-preserving text. Sent images are named buttons. [Quote display](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L87), [image display](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L733). |
| Retry and history | Retry resends the old string, reopening current file paths. Recall calls Composer.set. `cockpit/transcript_glue.rs:169-190`; `cockpit.rs:6641-6649`. | Failed send restores a structured snapshot when no newer draft exists. [Failure restoration](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1404). |
| Restart | Existing-thread draft stores prompt text and reconstructs quote/file records. Literal prefix collisions change editing roles. `cockpit/drafts.rs:28-56`; `composer.rs:506-511`. | Draft persistence keeps text, attachment identity and context. File metadata can explicitly require reattachment. [Persistence](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2291), [reattach state](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L462). |

Paths in this table without a crate prefix are relative to `crates/ferrite-pane/src/`, except `cockpit.rs` and `cockpit/`, which are relative to `crates/ferrite-cockpit/src/`.

## Source map

| Responsibility | Ferrite | T3 |
| --- | --- | --- |
| Selection and dispatch | `crates/ferrite-pane/src/transcript/reply.rs`; `crates/ferrite-cockpit/src/cockpit.rs:2127-2134` | [AssistantSelectionToolbar](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L23), [ChatComposer](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatComposer.tsx#L2989) |
| Quote storage and editing | `crates/ferrite-pane/src/quote.rs`; `crates/ferrite-pane/src/composer.rs:494-554,645-654` | [Citation schema](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/assistantCitations.ts#L11), [comment editor](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L10) |
| Context rendering | `crates/ferrite-pane/src/attachments.rs:149-295,391-618`; `crates/ferrite-pane/src/pane.rs:3536-3658` | [Citation chip](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L60), [attachment strip](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L135), [user message](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L443) |
| Provider boundary | `crates/ferrite-core/src/prompt_files.rs`; provider wire modules in finding 007 | [Citation provider expansion](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L148), [separate dispatch attachments](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1292) |
| Restore and image metadata | `crates/ferrite-cockpit/src/cockpit/drafts.rs`; `crates/ferrite-pane/src/pane.rs:3861-3955` | [Draft store](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2263), [intake preparation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L247) |

## Detailed findings

### quote-attachments-001. Cite has no reachable per-quote comment editor

Priority P1. Category `missing-capability`. Evidence `source-proven`.

The user cannot attach their own words to an individual citation. Multiple citations cannot each receive an independently editable instruction. The ordinary composer remains a separate place to type general text; this finding does not establish that all typing after Cite is broken.

Trigger. Select a passage, choose Cite, then try to add or edit a comment on that quote. Repeat with two quotes.

Expected behavior. Each citation offers an editable comment, preserves the surrounding draft, and makes its association clear before and after sending.

Actual behavior. Cite, Attach message as context, and terminal selection attachment create Reply with an empty text field. The tray renders a read-only div and an optional remove button. Composer exposes append, read and remove operations, but no comment editor or mutation path. Tests and a visual fixture inject nonempty Reply.text directly; they do not supply the missing user interaction.

Cause. The data model and renderer support preloaded comments, while the authoring workflow was never connected.

Ferrite evidence:

- `crates/ferrite-pane/src/transcript/reply.rs:206-220`.
- `crates/ferrite-pane/src/transcript/message_menu.rs:168-171`.
- `crates/ferrite-pane/src/attachments.rs:551-618`.
- `crates/ferrite-pane/src/composer.rs:409-419,645-654`.
- `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:130-133`.
- `crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:954-975`.
- `crates/ferrite-cockpit/src/cockpit/visual_reference/feature_scenes.rs:560-561`.

Current T3 evidence:

- [apps/web/src/components/chat/AssistantCitationChip.tsx:275](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L275).
- [apps/web/src/components/chat/AssistantCitationCommentEditor.tsx:81](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L81).
- [apps/web/src/components/ComposerPromptEditorTiptap.tsx:455](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L455).
- [apps/web/src/components/chat/ChatComposer.tsx:2989](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatComposer.tsx#L2989).

Recommendation. Add a native comment editor to the citation interaction, with Save, Cancel, multiline input, a focus return target and a later Edit comment action. Keep quote source and user comment separate. Correct composer.md's positive claim that reply cards already provide a usable commenting workflow.

Observable acceptance check. Using only UI actions, cite two different passages, add different multiline comments, edit one, cancel another edit, add general prompt text, reopen the draft and send. Each comment remains editable and paired with its quote. IME Enter does not save or send prematurely. A quote with no comment also sends.

### quote-attachments-002. Quote previews hide their full contents with no disclosure

Priority P2. Category `interaction`. Evidence `source-proven`.

The user cannot inspect the whole passage they are about to cite, and a sent prompt cannot reveal the complete quote or a preloaded long comment. Similar openings become indistinguishable.

Trigger. Cite a paragraph longer than three short lines, or restore a quote with a long comment. Inspect it before and after sending.

Expected behavior. A compact preview has a reachable full-text disclosure or source action. User-authored comments remain fully readable.

Actual behavior. The draft quote is an 88 by 88 pixel tile with 13 pixel type and a three-line clamp. When a comment exists, that comment replaces the quote in the tile. The sent Quote row clamps both the passage and comment to two lines. Neither renderer provides expand, full-text tooltip, preview or source navigation. The general prompt Show more calculation excludes leading quotes.

Cause. Image-tile geometry was reused for prose, and clipping became the only display mode.

Ferrite evidence:

- `crates/ferrite-pane/src/attachments.rs:505-543,551-618`.
- `crates/ferrite-ui/src/theme.rs:756-757,1974-1978,2100-2102,2604-2610`.
- `crates/ferrite-pane/src/pane.rs:3528-3533,3549-3559`.

Current T3 evidence:

- [apps/web/src/components/chat/AssistantCitationChip.tsx:87](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L87).
- [apps/web/src/components/chat/AssistantCitationChip.tsx:106](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L106).
- [apps/web/src/components/chat/AssistantCitationChip.tsx:255](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationChip.tsx#L255).

Recommendation. Use a text-sized citation row or chip with full inspection and comment editing. Preserve compactness through disclosure rather than irreversible clipping. Restore provenance under transcript-006; this finding separately covers local readability even when a source is unavailable.

Observable acceptance check. A 5,000-character citation, two citations with identical opening words, a code excerpt and a multiline comment can each be inspected in full before and after send. The UI clearly distinguishes source text from comment text at narrow pane widths. Screenshot review confirms readable wrapping and controls.

### quote-attachments-003. The Cite toolbar is pointer-only and does not observe keyboard selection

Priority P2. Category `interaction`. Evidence `source-proven`.

A keyboard user cannot reach or activate the displayed Cite action. A selection changed without mouse release does not create a fresh offer.

Trigger. Select transcript text, press Tab to reach Cite, then try Enter or Space. Also change a selection using keyboard navigation where supported.

Expected behavior. Cite is a named focusable control. Keyboard selection and pointer selection share a usable activation path.

Actual behavior. The offer is populated only by transcript mouse-up. glass_button returns a raw div. Cite attaches only an on_mouse_down handler, with no focus handle, tab stop, keyboard action or accessible label. The separate context-menu action attaches the entire message and is not an equivalent selected-passage operation.

Cause. The implementation solved pointer selection loss by acting on mouse-down but did not supply a semantic control or keyboard selection subscription.

Ferrite evidence:

- `crates/ferrite-pane/src/transcript.rs:3193-3195`.
- `crates/ferrite-pane/src/transcript/reply.rs:59-96,206-224,352-396`.

Current T3 evidence:

- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:140](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L140).
- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:148](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L148).
- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:191](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L191).

Recommendation. Give Cite a focusable native control and explicit keyboard activation while retaining a captured selection until activation settles. Observe selection changes independently of mouse-up. Keep Escape dismissal and the oversize explanation.

Observable acceptance check. Select a passage, Tab to Cite, activate with Enter and with Space, and confirm exactly one citation and no message send. Keyboard-only selection exposes the same action. Escape restores a predictable focus target without interrupting the agent.

### quote-attachments-004. Attach message as context silently does nothing for long messages

Priority P2. Category `defect`. Evidence `source-proven`.

The visible attach action can produce no attachment, error or explanation for a long assistant answer. This is a concrete no-op that can resemble the reported citation regression.

Trigger. Open the message context menu on an answer whose trimmed Markdown exceeds 10,000 characters and choose Attach message as context. A one-character message also fails the lower bound.

Expected behavior. The action either attaches an appropriate bounded reference or explains its limit before or after activation.

Actual behavior. menu_rows enables Attach whenever quotes are allowed, without inspecting message length. run_verb passes the whole Markdown through passage and emits only when it returns Some. passage rejects lengths outside 2 through 10,000. The selection toolbar has Shorten selection feedback, but the context menu does not.

Cause. The full-message action inherits a selection-length validator without its disabled state or feedback.

Ferrite evidence:

- `crates/ferrite-pane/src/transcript/message_menu.rs:103-123,168-171`.
- `crates/ferrite-pane/src/quote.rs:13-26`.
- `crates/ferrite-pane/src/transcript/reply.rs:225-227`.

Current T3 evidence:

- [apps/web/src/components/chat/TimelineAssistantRows.tsx:220](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineAssistantRows.tsx#L220).
- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:179](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L179).

Recommendation. Use a whole-message source reference, or show a disabled reason and offer a shorter selected passage. A successful-looking action must not disappear silently.

Observable acceptance check. Exercise 1, 2, 10,000 and 10,001 trimmed characters through the actual menu. Each action either creates context or displays an explicit reason. Unicode bounds match the explanation.

### quote-attachments-005. Ordinary blockquote text is reclassified as an uneditable citation on restore

Priority P2. Category `defect`. Evidence `source-proven`.

An ordinary prompt can change into attachment cards when reopened or recalled. Its text leaves the main editor, and its editing affordances disappear. This is a structure and editability defect; the words usually remain serialized.

Trigger. Type a draft beginning with > ordinary Markdown, persist it and reopen it. Separately cite a passage, then begin the general prompt with the literal marker ↩ followed by a space.

Expected behavior. Only context deliberately created as a citation becomes a citation. Literal text survives save, restore, recall and resend with its authoring role intact.

Actual behavior. quote::split consumes every leading all-blockquote paragraph as Reply, including ordinary typed text. A following paragraph beginning with ↩ is consumed as the previous reply's comment. Composer.set always invokes this parser after stripping attachment trailers. Draft persistence stores only Composer.prompt, so the restored parser cannot distinguish author intent. compose also replaces newline characters in comments with spaces.

Cause. The same untagged string is both user content and the storage grammar. There is no escape or versioned record boundary for quotes.

Ferrite evidence:

- `crates/ferrite-pane/src/quote.rs:67-118`.
- `crates/ferrite-pane/src/composer.rs:494-512,536-554`.
- `crates/ferrite-cockpit/src/cockpit/drafts.rs:28-56`.
- `crates/ferrite-cockpit/src/cockpit.rs:6641-6649`.

Current T3 evidence:

- [packages/shared/src/assistantCitations.ts:21](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L21).
- [packages/shared/src/assistantCitations.ts:43](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L43).
- [packages/shared/src/assistantCitations.ts:118](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L118).

Recommendation. Persist typed citation records or an unambiguous versioned encoding. Keep literal Markdown literal. Preserve multiline comments and support legacy prompt decoding without treating every new leading blockquote as structured context.

Observable acceptance check. Round-trip literal leading blockquotes, nested quotes, the literal ↩ marker, blank lines, Unicode and deliberate citations through draft restart, history and resend. Compare both text and editable record types. General text remains in the general editor.

### quote-attachments-006. A quote-only prompt renders differently when a file is attached

Priority P2. Category `defect`. Evidence `source-proven`.

Adding an unrelated file can turn the citation into raw > text in the sent prompt. Draft restoration reconstructs the quote differently from transcript rendering. Imported comments can also absorb the attachment trailer and hide the file preview.

Trigger. Cite one passage, add a PNG or ordinary file, leave the main editor empty and send. Also inspect a restored Reply with a nonempty comment and a file but no main text.

Expected behavior. Quote, comment and file rendering is independent of whether general prompt text exists. The same parser output drives draft and transcript.

Actual behavior. quote::compose emits > passage with no trailing blank paragraph when the main text is empty. prompt_files::compose appends the file token with a single newline. The resulting > passage newline @"/path" paragraph is not entirely quoted. Both prompt_row and prompt_text_chipping call quote::split before the relevant file trailer has been removed, so the quote is not recognized. Composer.set uses the opposite order and restores it. A marked-comment paragraph can instead consume its appended file token into Reply.text.

Cause. The transcript applies the nested text codecs in an inconsistent order. prompt_row strips files, then recomposes them before prompt_text_chipping parses the quote.

Ferrite evidence:

- `crates/ferrite-pane/src/quote.rs:74-85,91-118`.
- `crates/ferrite-core/src/prompt_files.rs:15-24`.
- `crates/ferrite-core/src/browser_annotations.rs:475-478`.
- `crates/ferrite-pane/src/composer.rs:506-511`.
- `crates/ferrite-pane/src/pane.rs:3226-3241,3543-3557`.

Current T3 evidence:

- [apps/web/src/components/chat/useComposerSend.ts:1292](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1292).
- [apps/web/src/components/chat/TimelineUserMessageRow.tsx:483](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L483).

Recommendation. Decode a prompt once into text, quotes, files and annotations, then render that result. Do not repeatedly split and recompose transport syntax inside render helpers.

Observable acceptance check. Test the cross-product of zero or nonempty main text, one or several quotes, zero or multiline comments, file/image attachments and browser annotations. Sent, reopened and recalled forms retain the same quote rows, file previews and comment boundaries. No transport token appears as comment prose.

### quote-attachments-007. Quoted file and skill syntax can become active provider input

Priority P1. Category `defect`. Evidence `source-proven`.

Citing reference text can attach a local file or activate a Codex skill even though the user only meant to discuss the quoted text. A quote can therefore change the provider input beyond its visible citation card.

Trigger. Cite a passage containing a whitespace-delimited @"/absolute/existing/image.png" or @"/absolute/existing/document.pdf". With Codex, cite text containing a discovered /skill name. Send without explicitly adding that file or skill.

Expected behavior. Quoted source text stays reference material. Only explicit composer context records or deliberate user-authored mentions produce attachment and skill input.

Actual behavior. quote::compose prefixes each line with > but leaves @ and / tokens intact. prompt_files::paths scans whitespace-delimited tokens across the whole prompt without considering quote context. Claude reads matching image/PDF files into base64 blocks. Codex adds localImage, localAudio or mention items and independently discovers skill names in the same text. The browser-annotation encoder already escapes @ in captured page text specifically to avoid this class of activation.

Cause. Provider adapters infer executable context from a flattened string after source text and user instructions have been combined.

Ferrite evidence:

- `crates/ferrite-pane/src/quote.rs:34-47,74-85`.
- `crates/ferrite-core/src/prompt_files.rs:49-93`.
- `crates/ferrite-core/src/providers/claude/wire.rs:25-62`.
- `crates/ferrite-core/src/providers/codex/wire.rs:1242-1293`.
- `crates/ferrite-core/src/browser_annotations.rs:453-461`.

Current T3 evidence:

- [packages/shared/src/assistantCitations.ts:148](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/assistantCitations.ts#L148).
- [apps/web/src/components/chat/useComposerSend.ts:1110](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1110).
- [apps/web/src/components/chat/useComposerSend.ts:1292](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerSend.ts#L1292).

Recommendation. Carry typed context to provider adapters and expand quotation text only as text. At minimum isolate quoted material from attachment and skill scanning. Use the annotation encoder's explicit separation as an existing design precedent, not a blanket text replacement that corrupts quoted content.

Observable acceptance check. Against a fake provider and harmless fixture files, cite exact file/skill syntax and inspect the wire payload. It contains the unchanged reference text but no unsolicited file bytes, localImage, localAudio, mention or skill item. Explicitly attaching the same fixture still produces the intended typed item.

### quote-attachments-008. Valid image attachments can show a permanent false Failed tile

Priority P2. Category `defect`. Evidence `source-proven`.

An image supported by the rendering library can appear broken before it is decoded. A repaired or newly created file can remain Failed until the app process restarts.

Trigger. Attach a valid SVG, AVIF or TIFF. Also attach a missing PNG, render its Failed tile, then create a valid PNG at the same path and reattach it.

Expected behavior. Image decode support, dimensions and error state agree. Unknown dimensions do not mean decode failure, and a changed file can recover.

Actual behavior. is_image accepts the broad GPUI extension list, including SVG, AVIF and TIFF. thumbnail treats image_facts == None as a hard failure and does not create gpui::img. image_facts uses a custom header reader for only PNG, GIF, BMP, WebP and JPEG, limited to the first 64 KiB. Its path-only cache stores both success and None forever, with no invalidation path. Reattaching the same path does not invalidate it.

Cause. Optional metadata is being used as an authoritative decode result, and its cache has no file-version key or retry.

Ferrite evidence:

- `crates/ferrite-pane/src/image_viewer.rs:65-72`.
- `vendor/gpui-pre/src/elements/img.rs:210-217`.
- `crates/ferrite-pane/src/attachments.rs:451-498`.
- `crates/ferrite-pane/src/pane.rs:3861-3896,3919-3955`.

Current T3 evidence:

- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:220](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L220).
- [apps/web/src/components/chat/useComposerAttachmentIntake.ts:273](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L273).

Recommendation. Let the supported decoder determine displayability. Use a neutral dimension fallback while metadata is unknown. Key cached facts by a file version or explicit attachment identity and expose a retry after changes. This is separate from composer-07's provider readiness problem.

Observable acceptance check. Valid SVG, AVIF, TIFF, PNG and a JPEG with large metadata each show a preview or an accurate format-specific explanation. A missing/corrupt file repaired at the same path recovers without restarting. Replacing an image updates dimensions and caption.

### quote-attachments-009. Draft file previews and sent image previews lack keyboard activation

Priority P2. Category `interaction`. Evidence `source-proven`.

A keyboard user can remove a draft file but cannot activate its preview. Image preview is keyboard-capable in the draft, then loses that capability after the message sends.

Trigger. Attach a text file and traverse the tray with Tab. Send a PNG, then traverse transcript controls and try opening its thumbnail with Enter or Space.

Expected behavior. Every preview affordance is a named focusable control before and after send, with the same activation semantics.

Actual behavior. Non-image draft files are raw clickable divs. Only their remove child is a tab stop. image_card uses a proper button in the draft, while inline_image wraps thumbnail as a raw clickable div for a sent message. The sent image has no tab stop, focus handle, keyboard context or accessible action label.

Cause. Equivalent attachment actions use different control implementations according to presentation location.

Ferrite evidence:

- `crates/ferrite-pane/src/attachments.rs:191-257,409-424,713-727`.
- `crates/ferrite-pane/src/pane.rs:3800-3838`.

Current T3 evidence:

- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:470](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L470).
- [apps/web/src/components/chat/TimelineUserMessageRow.tsx:733](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L733).

Recommendation. Share a semantic preview control between draft and transcript renderers, with a stable accessible name and focus restoration when preview closes. Keep removal a separate control.

Observable acceptance check. Open and close text and image previews entirely by keyboard before and after send. Every activation opens once without sending a message. Tab exposes preview and remove as distinct labeled actions. An accessibility inspection sees buttons rather than anonymous clickable text.

### quote-attachments-010. Attachment history stores mutable paths rather than retained content

Priority P2. Category `intentional-difference`. Evidence `source-proven`.

Retry and history can refer to changed or deleted file content. A pasted screenshot depends on a temporary file even after its prompt has been saved. The visual record is not a reliable record of the bytes originally sent. ADR 0004 explicitly chooses path storage, so this is a documented product tradeoff with incomplete user feedback, not an accidental omission of a promised byte store.

Trigger. Attach a local screenshot and send. Replace or delete that path, reopen the thread and retry. For clipboard images, remove only a disposable test image's scratch file before restoring its saved draft.

Expected behavior. An attachment records durable identity and sent content, or is clearly labeled as a live path reference with change/missing detection. Retry reports whether it uses original or current content.

Actual behavior. The composer stores PathBuf values, and prompt persistence stores only @path strings. Clipboard images are spilled under a per-process temp directory. Transcript previews reopen those paths. Retry clones the last prompt string, then provider adapters read the current file or omit an unreadable one. No general attachment asset copy is created by these intake paths.

Cause. ADR 0004 deliberately keeps paths in the existing text-based queue and persistence. The UI uses the attachment presentation for both live references and snapshots. Plain prompt durability does not provide attachment-byte durability.

Ferrite evidence:

- `crates/ferrite-pane/src/composer.rs:572-582,2104-2124`.
- `crates/ferrite-core/src/prompt_files.rs:15-24`.
- `crates/ferrite-cockpit/src/cockpit/drafts.rs:28-56`.
- `crates/ferrite-pane/src/pane.rs:3800-3836`.
- `crates/ferrite-pane/src/preview.rs:271-290`.
- `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:169-190`.
- `crates/ferrite-core/src/providers/claude/wire.rs:28-62`.
- `crates/ferrite-core/src/providers/codex/wire.rs:1276-1293`.

- `docs/adr/0004-prompt-attachments.md:23-36`.

Current T3 evidence:

- [apps/web/src/components/chat/useComposerAttachmentIntake.ts:204](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useComposerAttachmentIntake.ts#L204).
- [apps/web/src/lib/attachmentUploadQueue.ts:135](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135).
- [apps/web/src/composerDraftStore.ts:2291](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/composerDraftStore.ts#L2291).
- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:462](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L462).

Recommendation. Distinguish live file references from retained attachments. Copy clipboard media and explicit attachment snapshots into an owned durable store and keep content identity with history. Detect stale references and expose reattach/retry instead of silently changing content. Keep workspace @mentions live if that is the intended contract.

Observable acceptance check. Send version A of a fixture, replace the original with B, reopen and inspect history, then retry. The UI and provider payload agree on A versus B and never silently substitute. A pasted screenshot survives restart and routine temp cleanup, or displays an actionable missing-asset state before send.

### quote-attachments-011. The attachment shelf has no bounded overflow policy

Priority P2. Category `interaction`. Evidence `inferred`.

At narrow pane widths, many attachments can consume the available conversation height. Source shows unbounded growth; the installed app must establish the exact clipping or displacement symptom.

Trigger. Add enough images and citations to wrap across many rows in a small conversation pane, then try editing, sending, inspecting and removing every attachment.

Expected behavior. The composer retains an accessible editor and primary controls. A bounded scroll, compact summary or explicit expansion keeps every attachment reachable.

Actual behavior. Every image and reply is rendered as a fixed 88 pixel card. The card container wraps without a maximum height or scroll. add_files and reply impose no count bound. The shelf and composer block are flex_shrink_0. The inspected shelf has no collapsed summary or Show more control.

Cause. The layout assumes a small number of attachments. There is no overflow contract between shelf and transcript.

Ferrite evidence:

- `crates/ferrite-pane/src/attachments.rs:275-295,579-585`.
- `crates/ferrite-pane/src/composer.rs:572-582,647-649`.
- `crates/ferrite-pane/src/pane/composer.rs:501-510,564-577`.

Current T3 evidence:

- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:179](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L179).
- [apps/web/src/components/chat/ComposerAttachmentStrip.tsx:583](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L583).
- [apps/web/src/components/composerFooterLayout.ts:1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/composerFooterLayout.ts#L1).

Recommendation. Measure the installed layout with many attachments, then provide a bounded attachment region or compact summary with explicit expansion. Preserve navigation, removal and the active editor. Do not copy T3's expanded wrapping blindly; its resting summary and screenshot horizontal scrolling are the relevant comparison.

Observable acceptance check. At 360 pixel pane width and 600 pixel window height, add 12 images, six citations and long-named files. The editor, Send and all attachments remain reachable by pointer and keyboard. Resize and remove items without losing focus or the visible edit position. Capture before/after screenshots.

### quote-attachments-012. The reported pointer Cite regression still needs installed-app reproduction

Priority P1. Category `verification-gap`. Evidence `unverified`.

The user reports that citing no longer works. Existing event handlers and fixture tests do not establish that the installed app accepts the real selection and reaches the composer.

Trigger. Use the installed Ferrite build to select live assistant prose and click Cite, including after opening the right panel or switching panes.

Expected behavior. A visible Cite action creates exactly one citation in the intended composer and leaves text entry usable.

Actual behavior. Source captures selected text after mouse-up, gates on focused and quotes, dismisses when selection disappears, and dispatches on mouse-down. Cockpit appends the reply and relies on the focus pump. A test covers a simple fake-provider drag and click. This worker did not drive the app, and cannot identify a universal pointer failure from these handlers alone.

Cause. Unresolved. Candidate boundaries are focused-pane state, selection lifetime, deferred overlay hit-testing and focus handoff. None is established as the installed regression's cause.

Ferrite evidence:

- `crates/ferrite-pane/src/transcript/reply.rs:65-95,163-171,204-224`.
- `crates/ferrite-cockpit/src/cockpit.rs:2127-2134`.
- `crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:512-615`.

Current T3 evidence:

- [apps/web/src/components/chat/AssistantSelectionToolbar.tsx:185](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantSelectionToolbar.tsx#L185).
- [apps/web/src/components/chat/ChatComposer.tsx:2544](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatComposer.tsx#L2544).

Recommendation. Parent should capture the installed build identity, selection, toolbar, click result and focus destination. Keep the report open until that reproduction distinguishes no offer, dead activation, wrong destination and inability to type. Do not mark the regression fixed because a handler exists.

Observable acceptance check. On the installed build, test plain prose, code, tables, links, double/triple click, cross-block selection, scrolling, a narrow pane, right-panel focus and two open threads. Verify the citation reaches the intended draft and typing works. Record pointer coordinates and before/after evidence for every failure. Do not send to a live provider without an appropriate disposable thread.

## Duplicates and corrections to earlier slices

- `transcript-006` already owns missing source identity and exact message navigation. This pass confirms that `Reply` loses thread/message/range identity and that quote rows cannot open their source. Do not count that omission again. Findings 001 and 002 cover the distinct comment-authoring and full-text inspection failures.
- `composer-03` owns local paths presented as attachments in remote threads. `composer-07` owns provider readiness and silent native-image fallback. Finding 008 concerns false UI failure before decoding; finding 010 concerns historical content identity. These need coordinated fixes, not duplicate tickets for the same remote upload failure.
- `composer-08` owns large text pastes and input length validation. Clipboard screenshot write failure is another feedback case for that intake/readiness work. `spill_pasted_image` logs and returns None at `crates/ferrite-pane/src/composer.rs:2114-2122`; no attachment-specific UI error is returned. Do not count it twice here.
- `composer-09` owns inline positioning, ordering and rich context clipboard records. The shelf groups replies before images and files independently of intake order at `crates/ferrite-pane/src/attachments.rs:167-191,260-295`. The resulting inability to place comments around specific files belongs there.
- `composer-11` owns loss of structured review controls after draft reopen. `panel-tools-008` owns richer PDF/media/document preview coverage. This pass confirms ordinary attachment preview only chooses image or UTF-8/binary/unreadable at `crates/ferrite-pane/src/preview.rs:271-290`.
- `input-focus` owns general focus traversal and panel keyboard behavior. Findings 003 and 009 add concrete citation and attachment controls not enumerated in that report. They should join the same semantic-control repair effort without losing their acceptance checks.
- `composer.md` says Ferrite supports reply cards with comments. Replace that with a precise statement that the serializer and renderer accept comments, while no user-facing per-citation editor exists. Its cited `Composer::reply` append method is not evidence of that workflow. The three-replies test supplies comments directly at `crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:960-975`.
- Source-aware side chat/subagent quoting is a separate product decision. Side-chat transcript inputs set quotes false at `crates/ferrite-cockpit/src/cockpit/side_chat.rs:529,566`. This pass does not misclassify an intentionally absent composer on a subagent tab as the main Cite regression.

## Supported current T3 capabilities

T3's citation path includes source message identity, selected offsets and surrounding text, insertion at the current caret, optional comment creation and later editing, multiline comments, Cancel, save-and-send, focus return, preservation during controlled draft replacement, source navigation, and distinct provider serialization. Its comment UI guards IME Enter and length limits. These capabilities are supported by the citations under findings 001, 003 and 007 and [draft preservation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ComposerPromptEditorTiptap.tsx#L1548). They are source coverage, not runtime checks performed in this audit.

T3's attachment intake records File objects and metadata, prepares images, stages bytes to an environment, reports preparation/upload failure, supports retry and makes unavailable restored files explicit. The visual recipe includes compact resting previews capped at three plus an overflow count, expanded 64 pixel tiles, horizontal screenshot overflow, named file rows with size/status, source-aware screenshot details, and keyboard-operable sent previews. See [resting preview](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L583), [expanded media](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L179), [file list](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L455) and [upload transport](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/attachmentUploadQueue.ts#L135).

These details do not make every T3 choice mandatory. Native Ferrite can retain its own compact appearance. It still needs complete inspect, annotate, remove, restore and send behavior, and a readable account of what the provider receives.

## Positive Ferrite mechanisms to retain

These are source mechanisms, not declarations that the installed features work end to end.

- Cite captures the selection before the activation press can clear it, and does not immediately send a prompt. The cockpit appends to the existing draft. See `crates/ferrite-pane/src/transcript/reply.rs:75-85,204-220` and `crates/ferrite-cockpit/src/cockpit.rs:2127-2134`.
- Quote transport normalizes selection line endings and prefixes every source line, including empty lines. It preserves ordinary main text after the citation. See `crates/ferrite-pane/src/quote.rs:20-47,74-85`. Finding 007 explains why that text quoting is not sufficient to isolate provider input.
- File tokens use JSON string escaping. Paths with spaces, quotes, Unicode and backslashes do not require shell parsing. See `crates/ferrite-core/src/prompt_files.rs:8-12,60-76`.
- Clipboard paths take precedence over duplicate path text. Screenshots get an atomic scratch write before their path is attached. See `crates/ferrite-pane/src/composer.rs:594-619,2114-2120`. Durability and error feedback remain incomplete.
- Draft image preview and remove controls are actual tab stops, and browser annotations already have a named review button. The image gallery gives a full preview beyond the crop. See `crates/ferrite-pane/src/attachments.rs:329-368,409-424,637-659` and `crates/ferrite-pane/src/preview.rs:68-80`.
- Selection scroll-dismiss motion honors reduced motion at `crates/ferrite-pane/src/transcript/reply.rs:98-119`. File chips use the existing insertion motion at `crates/ferrite-pane/src/attachments.rs:139-146`. Reply cards bypass that grown wrapper at `:260-264`; the motion report owns the desired insertion/removal treatment.

## Visual review criteria and remaining runtime checks

The parent owns screenshots and live interactions. No captured image was inspected by this worker. The following distinctions prevent aesthetic opinions from being presented as measurements.

| Concern | Source-established fact | Runtime evidence still needed |
| --- | --- | --- |
| Quote density | 88 pixel square, three draft lines, two sent lines, no disclosure. | Actual wraps at 360, 600 and 900 pixel pane widths, especially code and identical quote prefixes. |
| Contrast | Draft quote text uses INK_SUBTLE at 13 pixels on RAISED. Sent quote text uses INK_STRONG. `attachments.rs:518-540,582-590`. | Inspect current theme, scaling and focus/hover states. No numerical contrast failure is asserted. |
| Screenshot crop | Draft images use ObjectFit::Cover in a square. Sent dimensions preserve aspect within fixed caps. `attachments.rs:492-496`; `pane.rs:3808-3822`. | Wide code screenshots, tall screenshots, tiny images and diagrams. Verify useful preview detail and full-image opening. T3 also crops compact previews. |
| Labels and identity | Non-image draft chips show only a truncated basename; full path is on the remove tooltip. `attachments.rs:191-245`. | Two identical basenames from different directories, long Unicode filenames and missing files. Confirm the user can distinguish them without risking removal. |
| Overflow | Shelf wraps all cards with no local height cap; composer does not shrink. Finding 011. | Measure viewport loss and reachability with many attachments. Do not report an exact clipped region without the measurement. |
| False failure | The header reader can reject supported image formats and caches None. Finding 008. | Capture a valid SVG tile and same-path recovery. Separate preview failure from provider format support. |
| Pointer Cite regression | Event path and simple fake-provider test exist. Finding 012. | Installed-build selection, overlay hit-testing and focus handoff in the user's actual layout. |

An efficient regression sequence starts with one live Cite plus typing, then two citations with different comments, then a quote-only send with a file. Use disposable fixtures for provider-input scanning, literal-codec round trips, changed-file retry and failed-image recovery. Finish with keyboard-only preview and a narrow-window attachment stress case. Check exactly what is sent and restored, not just whether a card appears.

The existing simple selection test at `crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:512-615` is useful but cannot cover the missing comment editor, source identity, long-message no-op, mixed codec order, literal-prefix collisions, token activation, file mutation or format/cache errors. The preloaded-comment test at `:954-998` proves only a rendering path. Future regression tests should exercise user actions and the real serialization boundary for those cases.

Recommended work order is the citation comment flow and confirmed pointer regression, quoted-data isolation, one shared prompt decoder, durable attachment identity and readiness, then semantic controls and readable bounded presentation. Preserve existing typed drafts during every migration. A card that renders and a handler that exists are not completion criteria.
