# Transcript investigation

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants the functional gaps that remain after Ferrite's recent transcript visual fixes, with current T3 Code evidence and concrete checks. This report separates shipped source behavior from observed runtime behavior. No live UI was driven and no application files were changed.

Throughput checkpoint is not applicable to this read-only investigation.

## Overview

Ferrite HEAD is `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 HEAD is `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, inspected at `/tmp/ferrite-t3-audit-source-20261113`. Historical intent comes from `docs/design/t3-reference/transcript.md`, based on T3 `bcee0f5d`, and `docs/adr/0038-t3-code-as-the-transcript-reference.md`.

The old transcript gap list is materially stale. Ferrite now implements most of its structural requests. The strongest remaining gaps concern searching folded content, retaining readable plan history, preserving work-group bounds, and navigating references back to their source. Long-history paging needs a runtime check before treating the source-derived absence as conclusive. Math and media are separate parity opportunities. They are not requirements imposed by the work-row ADR.

There are eight issues below. Seven are source-proven and one is inferred. None is runtime-reproduced. Priorities are seven P2 items and one P3 item. No P0 or P1 incident is established by this slice.

## Key concepts

A Ferrite Transcript contains Blocks. Activity limits its retained rendering projection separately from the stored event log. TranscriptRows converts those Blocks into answer, prompt, group, fold, compaction and output rows. TranscriptView owns native list state and a selection document. Its copy and Find sources currently follow the same rendered-row projection.

A turn fold is different from a work-group disclosure. A closed turn removes intermediate work rows. Opening that turn exposes groups whose children may still be closed. These two levels explain why a visible final answer can coexist with undiscoverable intermediate content.

Working plan state is different from a saved plan artifact or proposed-plan approval. This report's plan-history issue concerns checklist updates such as update_plan and TodoWrite. It does not claim that plan approval is absent.

## Traced flows

| Flow | Source trace and outcome |
| --- | --- |
| Turn rendering | `crates/ferrite-pane/src/transcript/turns.rs:98-137` identifies turns and steer segments. `:153-170` preserves failed turns and final prose. `:200-243` creates Worked and Continued labels. `crates/ferrite-pane/src/transcript/rows.rs:549-559` removes members of closed folds. |
| Working state | `crates/ferrite-pane/src/pane/working.rs:122-169` builds separate header and live-tail state. `:145-149` enables live motion according to reduced-motion preference and attaches the current plan. |
| Group detail | `crates/ferrite-pane/src/transcript.rs:1783-1851` resolves group state and children. `crates/ferrite-pane/src/pane.rs:4384-4465` renders compact entries. `:4471-4529` implements scroll caps and edge fades, with the cap exception in issue 004. |
| Search | `crates/ferrite-pane/src/transcript.rs:1201-1247` collects rendered logical text without mounting every row. `crates/ferrite-pane/src/transcript/find.rs:77-119` searches that text. `:136-172` scrolls to the hit and settles the occurrence clear of the pinned prompt. |
| Selection and cite | Native selection receives all projected text members at `crates/ferrite-pane/src/transcript.rs:1211-1212`. `crates/ferrite-pane/src/transcript/reply.rs:206-220` stages a selected passage as a Reply. `crates/ferrite-pane/src/quote.rs:74-85` serializes it into the next prompt. |
| Edit and rewind | Prompt actions are visible only when prompt identity and provider capability are available at `crates/ferrite-pane/src/transcript.rs:1569-1587`. The gate rejects active turns, queued prompts, pending requests and provider changes at `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:29-46`. Edit immediately requests a conversation rewind with the old prompt restored to the composer at `:275-306`. Rewind opens the choice menu first. |
| Fork | Prompt fork delegates to core at `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:307-328`. Answer fork finds the next prompt to preserve the conversation through the chosen answer at `:331-343`. Source proves wiring, not successful provider execution. |
| Links | `crates/ferrite-pane/src/rich.rs:535-575` renders file and thread chips. `:605-645` routes normal link activation to scoped files, in-app URL preview or system URL opening. `crates/ferrite-pane/src/transcript/message_menu.rs:192-203` supports Open link, Copy link and Open in Files. Reported PR-specific failures belong to the PR navigation slice. |
| Changed files | `crates/ferrite-pane/src/transcript/outputs.rs:190-195` emits OpenChanges. `:231-270` expands folders and opens a selected file. Whether Changes shows the intended historical diff belongs to the panel-tools slice. |
| History | `crates/ferrite-core/src/activity.rs:324-337` defines limits. `:756-815` trims the rendering projection. `crates/ferrite-core/src/cockpit/mirror.rs:313-324` rebuilds it on history arrival. Main-transcript backward paging was not found. |

## Source map and historical specification reconciliation

| Historical request | Current Ferrite evidence | Assessment |
| --- | --- | --- |
| Top working header and default completed-turn fold | `crates/ferrite-pane/src/pane/working.rs:40-64`; `crates/ferrite-pane/src/transcript/turns.rs:200-243` | Implemented. Quick durations and steer labels exist. |
| Failures and questions survive folds | `crates/ferrite-pane/src/transcript/turns.rs:36-59`; `:153-154` | Implemented. Failed tools, question tools, notices, compaction and live tools are protected. |
| Work icons and reserved entry chevron | `crates/ferrite-pane/src/pane.rs:4268-4273`; `:4299-4309` | Implemented in the shared work-row path. |
| Plain capped expanded work list with fades | `crates/ferrite-pane/src/pane.rs:4471-4529` | Implemented until any child opens. Issue 004 covers the remaining behavior. |
| Group header without chevron | `crates/ferrite-pane/src/pane.rs:4419-4429` | Implemented through Slot::None. |
| Prompt clamp and actions | `crates/ferrite-pane/src/pane.rs:3525-3528`; `:3560-3606`; `crates/ferrite-pane/src/transcript.rs:1565-1606` | Implemented. Runtime overlap and keyboard checks remain. |
| Compaction divider | `crates/ferrite-pane/src/transcript.rs:2539-2574` | Implemented with title, token counts, live shine and failure reason. |
| Changed-file tree | `crates/ferrite-pane/src/transcript/outputs.rs:190-270` | Implemented with diff action and folder disclosure. |
| Thread link chips | `crates/ferrite-pane/src/rich.rs:553-560` | Implemented. Message-level provenance is a separate gap. |
| Cite at selection | `crates/ferrite-pane/src/transcript/reply.rs:195-220` | Implemented as a selected-text reply. Source navigation remains absent. |
| Background and subagents | `crates/ferrite-pane/src/pane/background_work.rs`; `crates/ferrite-pane/src/pane/agents.rs` | Present, audited by the dedicated worker. No duplicate routing findings here. |

## Detailed findings
### transcript-001. Find cannot discover text hidden by turn or tool disclosures

Priority P2. Category `partial-capability`. Evidence `source-proven`.

A reader can receive zero matches for text that exists in the loaded conversation. The new default turn folding makes this common for intermediate explanations and command output.

Trigger. Complete a turn containing a distinctive intermediate sentence or tool-output token. Leave its turn fold closed, open Find, and search for that token.

Expected behavior. Search the loaded conversation independently of disclosure state, then open the enclosing turn and tool when the reader visits a match.

Actual behavior. Find searches self.texts. That collection is built only from projected rows and the currently expanded details. Hidden turn members have no row; collapsed calls omit their output.

Cause. The copy projection doubles as the search index. Copying visible content and discovering hidden content need different projections.

Ferrite evidence. `crates/ferrite-pane/src/transcript/find.rs:3-9`; `crates/ferrite-pane/src/transcript/find.rs:99-111`; `crates/ferrite-pane/src/transcript.rs:1201-1247`; `crates/ferrite-pane/src/transcript/rows.rs:549-559`.

Current T3 evidence. [apps/web/src/components/chat/MessagesTimeline.tsx:639-662](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L639-L662); [apps/web/src/components/chat/MessagesTimeline.tsx:714-729](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L714-L729); [apps/web/src/components/chat/TimelineUserMessageRow.tsx:298-299](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L298-L299).

Recommendation. Build a search index from semantic messages and tool details. Keep visible-content selection separate. Map each hit to the disclosure chain and stable content identity.

Acceptance check. A token present only in a collapsed intermediate answer or collapsed command result is counted before expansion. Next match opens the required disclosures and reveals the exact occurrence. Closing Find preserves the reader's place.

### transcript-002. Long transcript retention has no equivalent of earlier-turn paging

Priority P2. Category `missing-capability`. Evidence `inferred`.

Readers of long sessions cannot reliably return to the earliest prompts or search them from the transcript. Stored events may still exist; this is a navigation and retrieval limitation, not a claim of data loss.

Trigger. Use a session exceeding the retained block or content-byte budget, then scroll to its first displayed row.

Expected behavior. Show that earlier history exists and load an older page while retaining the visible anchor. Expose loading, failure and retry states.

Actual behavior. The main transcript is a bounded projection. Defaults are 2,000 blocks and 4 MiB of retained content per subject. Old records and blocks are discarded from that projection. The inspected transcript input and event APIs provide no earlier-page cursor or request.

Cause. Retention is implemented independently of a pageable reading window. Replaying history rebuilds the same bounded Activity. Source inspection found no main-transcript older-page affordance.

Ferrite evidence. `crates/ferrite-core/src/activity.rs:324-337`; `crates/ferrite-core/src/activity.rs:756-815`; `crates/ferrite-core/src/transcript.rs:2952-2958`; `crates/ferrite-core/src/cockpit/mirror.rs:313-324`; `crates/ferrite-pane/src/transcript.rs:140-191`; `crates/ferrite-pane/src/transcript.rs:273-311`.

Current T3 evidence. [apps/web/src/components/chat/MessagesTimeline.tsx:1505-1534](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L1505-L1534).

Recommendation. Add a stored-history cursor and older-page loading to the main transcript. Preserve bounded memory with a sliding window. Show partial coverage rather than silently presenting the retained start as the conversation start.

Acceptance check. A fixture larger than both retention budgets can reach its original prompt through an earlier-history control. Prepending a page does not move the current visible sentence. A failed page load shows a retry action. Confirm the installed build before promoting this inferred gap to a reproduced defect.

### transcript-003. Successful turns erase the visible plan history

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Readers cannot check which planned steps were completed or left pending after a successful answer. Expanding Worked does not recover the checklist.

Trigger. Let a turn publish or update a checklist and finish successfully, including a plan with an unfinished step.

Expected behavior. Keep a historical, non-live plan record reachable inside the completed turn. A completed response must not erase unfinished checklist evidence.

Actual behavior. plan_of returns an empty plan for Completed, regardless of remaining steps. Checklist tools are StateOnly and their rows are suppressed unless they failed. Interrupted or failed latest turns retain a plan, which is useful but incomplete coverage.

Cause. The plan is treated as transient working state and the corresponding historical tool records are hidden.

Ferrite evidence. `crates/ferrite-pane/src/pane/working.rs:627-655`; `crates/ferrite-core/src/transcript.rs:331-335`; `crates/ferrite-pane/src/transcript/rows.rs:413-427`; `crates/ferrite-pane/src/pane/working.rs:816-843`.

Current T3 evidence. [apps/web/src/components/chat/MessagesTimeline.logic.ts:1966](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.logic.ts#L1966); [apps/web/src/components/chat/MessagesTimeline.logic.ts:1992](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.logic.ts#L1992); [apps/web/src/components/chat/V2ItemInspector.tsx:404-412](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/V2ItemInspector.tsx#L404-L412).

Recommendation. Snapshot the last checklist state per turn and render it as historical work. Retain the useful blocked presentation for interrupted turns, with no live spinner after settlement.

Acceptance check. After a successful turn ends, expand Worked and inspect the original ordered steps and their final statuses. A pending step remains visibly pending. The same historical plan survives reopening the thread.

### transcript-004. Opening one tool removes the entire work-group height cap

Priority P2. Category `interaction`. Evidence `source-proven`.

Inspecting one result in a large group can expand all its compact rows into the main transcript and displace surrounding content far beyond the intended bounded work list.

Trigger. Expand a group of many tool calls, then expand one short command result within it.

Expected behavior. Keep the compact-row viewport capped and add only the space required by opened details.

Actual behavior. The transcript sets opened when any child is expanded. group_list computes its cap only when opened is false, so any opened child removes max_h altogether.

Cause. A Boolean child-open state replaces measured added detail height.

Ferrite evidence. `crates/ferrite-pane/src/transcript.rs:1821-1848`; `crates/ferrite-pane/src/pane.rs:4485-4488`; `crates/ferrite-pane/src/pane.rs:4507-4514`.

Current T3 evidence. [apps/web/src/components/chat/TimelineWorkGroupRows.tsx:231-241](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineWorkGroupRows.tsx#L231-L241); [apps/web/src/components/chat/TimelineWorkGroupRows.tsx:274-281](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineWorkGroupRows.tsx#L274-L281).

Recommendation. Retain the base cap of min(288 px, half the viewport) and add measured detail height. Keep the group's scroll anchor when children open and close.

Acceptance check. With 100 compact calls, opening a two-line result grows the group only by that result's added height. The other compact calls remain internally scrollable. Closing the child restores the previous group position.

### transcript-005. Pin as chapter is a transient label without chapter navigation

Priority P3. Category `partial-capability`. Evidence `source-proven`.

The menu promises a way to mark a useful answer, but there is no chapter list to retrieve it and a new transcript view starts with no pins.

Trigger. Choose Pin as chapter on an answer, leave the answer far above the current viewport, then recreate the view or restart.

Expected behavior. A pin should be retrievable through chapter navigation and should survive normal thread reopening, or the action should state its temporary scope.

Actual behavior. Pin inserts the last BlockId into TranscriptView.pinned and renders Chapter beside the answer actions. It emits no persistence event. Construction initializes an empty HashSet. The module explicitly says chapters have nowhere to be listed.

Cause. The user-facing menu ships ahead of a chapter model, persistence and navigation.

Ferrite evidence. `crates/ferrite-pane/src/transcript/message_menu.rs:19-20`; `crates/ferrite-pane/src/transcript/message_menu.rs:173-177`; `crates/ferrite-pane/src/transcript.rs:483-484`; `crates/ferrite-pane/src/transcript.rs:611`; `crates/ferrite-pane/src/transcript.rs:1497-1503`.

T3 comparison. No T3 equivalent is asserted. This is an incomplete Ferrite affordance.

Recommendation. Either complete persistent chapter navigation using stable message identities or remove this incomplete menu action until its purpose is supported. This is Ferrite's own incomplete feature, not a requirement inferred from T3.

Acceptance check. Pin an answer, restart, open the same thread and use a chapter list to jump to it. If the feature is intentionally temporary, the action name and feedback explicitly say so.

### transcript-006. Citations and copied text do not retain a navigable source message

Priority P2. Category `partial-capability`. Evidence `source-proven`.

A cited passage reaches the model, but the reader cannot jump back to the original answer or copy a stable link to a specific prompt. Repeated text makes manual relocation ambiguous.

Trigger. Cite a passage, send the next prompt, then try to navigate from that quote to the original response. Try to copy a link to a prompt.

Expected behavior. Preserve source thread, message identity and selected range alongside quoted text. Provide a message-level link that restores the source position and opens enclosing folds.

Actual behavior. Reply stores only quote and text, then serializes ordinary blockquote text. Cite constructs Reply::new(passage, ""). Prompt actions provide edit, rewind, fork and copy text, with no prompt-link action. Existing ferrite-thread chips navigate at thread granularity.

Cause. Quote transport and message navigation share no stable source-reference data model.

Ferrite evidence. `crates/ferrite-pane/src/quote.rs:50-85`; `crates/ferrite-pane/src/transcript/reply.rs:206-220`; `crates/ferrite-pane/src/transcript.rs:1565-1593`; `crates/ferrite-pane/src/transcript.rs:303-305`.

Current T3 evidence. [apps/web/src/components/chat/TimelineAssistantRows.tsx:220-243](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineAssistantRows.tsx#L220-L243); [apps/web/src/components/chat/MessagesTimeline.tsx:664-706](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L664-L706); [apps/web/src/components/chat/TimelineUserMessageRow.tsx:834-840](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L834-L840).

Recommendation. Add optional structured provenance to Reply without breaking plain-text provider prompts. Add a prompt/message deep link and a source-opening action on sent citation chips.

Acceptance check. A citation made from a folded historical response opens the exact original passage after reopening the thread. A copied prompt link lands on that prompt. Duplicate passages resolve by message identity, not substring alone.

### transcript-007. Markdown math is displayed as code instead of typeset formulas

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Technical explanations containing fractions, matrices or aligned equations remain raw TeX-like text and are harder to read.

Trigger. Render an assistant answer containing inline math and a display equation.

Expected behavior. Typeset supported math while preserving the original source for selection/copy and showing a readable fallback during incomplete streaming or parse errors.

Actual behavior. The native Markdown parser maps InlineMath to a code-marked inline node and Math to an ordinary code block. Ferrite's custom code-block view adds Mermaid rendering, but no math renderer.

Cause. The document representation has no equation renderer in the inspected transcript path.

Ferrite evidence. `vendor/gpui-base/src/text/format/markdown.rs:233-237`; `vendor/gpui-base/src/text/format/markdown.rs:413-417`; `crates/ferrite-pane/src/rich.rs:647-655`.

Current T3 evidence. [apps/web/src/components/ChatMarkdown.tsx:1414-1421](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1414-L1421); [apps/web/src/components/chat/MarkdownMath.tsx:9-12](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownMath.tsx#L9-L12); [apps/web/src/components/chat/MarkdownMath.tsx:33-74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownMath.tsx#L33-L74).

Recommendation. Add an optional math rendering path with literal fallback and source-preserving copy. This is a capability choice beyond ADR 0038's narrower work-group contract, not evidence that the recent styling migration failed.

Acceptance check. Inline fractions and display matrices render correctly in both themes. Partial invalid streaming remains readable. Copy returns the source equation and wide formulas remain keyboard-scrollable.

### transcript-008. Transcript Markdown has no inline audio or video playback

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Generated or referenced audio and video require leaving the transcript. Image-style media references cannot become the inline players T3 provides.

Trigger. Receive Markdown with an audio or video destination in image syntax, or a media artifact intended for inline review.

Expected behavior. Offer bounded audio/video playback with a meaningful unavailable state, or make the unsupported media an explicit openable attachment.

Actual behavior. Native Markdown Image nodes always become ImageNode without media classification. The transcript-specific custom renderer handles link chips and Mermaid, with no inline audio/video player. Input attachment support for audio is a separate capability.

Cause. The native Markdown media representation is image-only.

Ferrite evidence. `vendor/gpui-base/src/text/format/markdown.rs:225-231`; `crates/ferrite-pane/src/rich.rs:535-575`; `crates/ferrite-pane/src/rich.rs:647-655`; `crates/ferrite-pane/src/pane.rs:3800-3832`.

Current T3 evidence. [apps/web/src/components/ChatMarkdown.tsx:1490-1518](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1490-L1518); [apps/web/src/components/chat/ChatMarkdownMedia.tsx:298-321](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatMarkdownMedia.tsx#L298-L321).

Recommendation. Classify media references before image rendering and provide accessible native playback or an intentional media attachment fallback. Treat this as optional parity investment according to the product's media workflows.

Acceptance check. An MP3 and an MP4 referenced in an assistant answer each expose playback controls or a labeled open action. Failed media does not appear as an unexplained broken image. Playback stops appropriately when its transcript is released.

## Positive Ferrite behavior

Ferrite has meaningful interaction coverage beyond the original visual comparison. Native list reconciliation applies splices and remeasurement without resetting the logical anchor at `crates/ferrite-pane/src/transcript/scroll.rs:311-328`. The resize path retains a fully visible row and its inset at `:236-270`. Tail following pauses for explicit navigation and resumes at the end at `:110-141`. These are positive source facts, not measured guarantees of smoothness.

Find searches offscreen projected content without mounting every row. It wraps forward and backward and positions the exact hit clear of the pinned band at `crates/ferrite-pane/src/transcript/find.rs:123-172`. This should be retained when fixing hidden-content search.

The context menu supports separate plain and Markdown copies at `crates/ferrite-pane/src/transcript/message_menu.rs:165-170`. Link context menus preserve the destination. The selected quote workflow has length limits and a Shorten selection affordance at `crates/ferrite-pane/src/quote.rs:13-30` and `crates/ferrite-pane/src/transcript/reply.rs:225-227`.

Tool details retain shell output, omitted-byte indicators and structured output at `crates/ferrite-pane/src/pane/output.rs:190-255`. Edits render inline diffs when expanded at `crates/ferrite-pane/src/pane.rs:4329-4335`. Image-reading tools can expose an image and gallery action at `:4322-4327`.

The Markdown implementation supports headings, lists, task lists, blockquotes, fenced code and tables, with renderer-level spacing coverage at `crates/ferrite-pane/src/rich.rs:2504-2544`. Wide tables have native overflow at `:2548-2571`. Images on separate lines get frames while badges remain inline at `:2574-2608`. Mermaid has an actual diagram renderer and source-code fallback at `crates/ferrite-pane/src/visual/diagram.rs:1-9`, rather than being absent as a generic native-rendering assumption might suggest.

The transcript has existing motion. Streaming text uses STREAM_FADE and STREAM_LAND at `crates/ferrite-pane/src/rich.rs:580-589`. Citation affordances fade on scroll and honor reduced motion at `crates/ferrite-pane/src/transcript/reply.rs:98-119`. Work-group expansion itself still snaps, documented at `crates/ferrite-pane/src/pane.rs:4250`. Current T3 also conditionally mounts group rows, but that does not settle the user's request for useful expansion motion. The dedicated motion report owns that recommendation.

## Supported current T3 capabilities

Current T3 has source-aware response references, prompt-link copying, automatic fold opening for references and Find, deferred work-history loading, earlier-turn paging with errors, bounded nested group scrolling that grows by added detail height, structured historical checklist detail, typeset math with source-preserving copy, and inline audio/video. The pinned citations in the findings point to their implementations.

T3 also guards rewind against unsupported providers, disconnected environments and running turns. Its rewind flow asks for the file-restoration choice before mutation at [useTimelineRewind.ts:109-144](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineRewind.ts#L109-L144). Ferrite's immediate Edit rewind differs from this behavior. The menu tooltip and restoration semantics need a live usability check before classifying that difference as a bug.

## Verification gaps and acceptance sequence

No tests were executed because this task changed only investigation artifacts. Existing test names are evidence of intended coverage, not green results from this audit. Runtime access belongs to the parent.

1. In a disposable thread, exercise Find against final prose, folded intermediate prose, collapsed command output and an offscreen expanded output. Existing coverage at `crates/ferrite-cockpit/src/cockpit/tests/find.rs:67`, `:129`, `:180` and `:266` covers offscreen matching, navigation and output positioning. It does not establish hidden-content discovery.
2. Inspect a group of 100 calls and open one child, including a short result and a tall result. Measure the group and outer-list position before and after. Source proves the cap disappears, but displacement was not measured.
3. Finish a checklist turn with one step still pending. Expand all work, switch threads and reopen. Confirm plan-history loss and compare to an interrupted turn.
4. Feed a disposable stored transcript beyond 2,000 blocks and 4 MiB. Confirm the configured limits, the first displayed row, whether any older-history retrieval exists outside the inspected APIs, and whether retained partial coverage is explained. Child-history retry exists at `crates/ferrite-cockpit/src/cockpit/subagents.rs:522-556`; it must not be mistaken for main-transcript backward paging.
5. Scroll upward while the answer streams, resize the right panel, expand a turn above the viewport, navigate away and back, then press Scroll to end. Existing tests include `crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:46` and `:132`, and `crates/ferrite-cockpit/src/cockpit/tests/send_scroll.rs:419`. Native anchor correctness needs the installed app, especially with image and diagram height changes.
6. Copy plain text and Markdown across a table, a code block, a link chip and a virtualized row boundary. Drag and click the same link, test middle click, and test the link context menu. The PR navigation worker owns PR destination correctness.
7. Exercise Edit, Rewind with and without files, prompt Fork and answer Fork against each actual provider. Check draft replacement, failure recovery, attachment preservation and the destination thread. The source gate does not prove provider-side rollback or checkpoint success.
8. Submit and answer single-choice, multi-choice and free-text questions, then inspect their completed transcript records. Turn folding protects question tools at `crates/ferrite-pane/src/transcript/turns.rs:43-46`. Pending-decision interaction belongs to the composer/provider workers. This slice did not reproduce decision-answer rendering failures.
9. Render valid and invalid Mermaid, inline/display equations, a remote image, a missing local image, MP3 and MP4. Verify source-preserving copy, error fallback, playback lifecycle and keyboard reachability.
10. Pin an answer, restart and search for a chapter-navigation entry. Cite a passage containing duplicate text, send it and try returning to its source. Check exact message identity rather than merely seeing quoted text.

Recommended order is hidden-content Find, historical checklist state, bounded work-group expansion, and a decision on pageable history. Then add reference provenance if cross-thread reading is important. Complete or remove the chapter affordance. Math and media follow according to the user's real content workflows. Runtime reproduction should precede fixes to disputed pointer, scroll and provider behaviors.
