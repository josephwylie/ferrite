# Draft lifecycle runtime correction

Three existing workflows were exercised in the rendered Ferrite app, including their return paths. The results show why a component or handler existing is insufficient evidence that the workflow works: a new draft disappears across a window restart; queued Edit cancels a message and combines it with a different draft; and Reopen closed surface restores an empty SideChat tab after destroying its content.

This supplements `composer-02` and `composer-04` in [composer.md](composer.md) and corrects any blanket claim that shared surface reopening is working. It does not change the aggregate tally or adjudicate unrelated surfaces. The runtime facts, exact command-log line references, and limits are also recorded in [draft-lifecycle-runtime-correction.json](draft-lifecycle-runtime-correction.json).

## Runtime, isolation, and evidence

- Existing production Cockpit in `target/debug/ferrite --headless`; source commit `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- Binary SHA-256 `8f47873505f48c90a267ef5b02913a345b72e390d17b78cd3a72c552497a2b98`.
- Fresh Store `/tmp/ferrite-audit-runtime-8ckr_nxp/threads`, separate working directory, settings, server, and headless app. Installed GUI and real Store were untouched.
- Fixed 1440×900 logical window; captures are 2880×1800 at scale 2. No CEF, application source changes, builds, Rust tests, or real model prompts.
- Fake Claude advertises version `9999.0.0` so discovery selects the fixture. Provider input/output uses the production Claude adapter. The final fixture announces `msg_lifecycle_v1`, accepts async messages by their actual UUID, and acknowledges cancellation. Its busy turn waits on a file controlled by this audit.
- All UI mutations use public `window key`, `window type`, and `window click`; selectors only locate rendered elements. Thread creation/focus uses public CLI. No direct call to a UI helper injected a desired result.

Raw [commands.jsonl](../evidence/draft-lifecycle-correction/commands.jsonl), [provider input](../evidence/draft-lifecycle-correction/provider-host.jsonl), [provider peer](../evidence/draft-lifecycle-correction/provider-peer.py), [driver](../evidence/draft-lifecycle-correction/drive.py), and [state](../evidence/draft-lifecycle-correction/state.json) are retained. Commands that establish each precondition are identified below. The initial startup New project modal was dismissed before valid draft testing; capture 01 is setup, not evidence of a typed draft.

## 1. A new unsent draft does not survive a window restart

**Runtime reproduced; existing `composer-02`.** A draft attached to a real thread survives, while text in a fresh Cmd+N pane does not.

Reproduction:

1. Start and focus fixture thread 1. Dismiss startup modal with Escape. Type `EXISTING_DRAFT_SENTINEL` in its composer. `thread get 1` reports `draft: true`; [capture 02](../evidence/draft-lifecycle-correction/02-existing-draft-before.png) shows the exact text.
2. `window key cmd-n`, then `window type UNSENT_DRAFT_SENTINEL`. [Capture 03](../evidence/draft-lifecycle-correction/03-unsent-draft-before.png) shows that exact text in the new draft.
3. Wait more than the normal one-second autosave period (the explicit final wait was two seconds, after several other observations). Copy the actual `drafts.json`: [drafts-before-restart.json](../evidence/draft-lifecycle-correction/drafts-before-restart.json) contains only `{"1":"EXISTING_DRAFT_SENTINEL"}`.
4. Terminate only the owned headless window process group and start the same binary with the same Store. Keep server and provider alive. [Capture 04](../evidence/draft-lifecycle-correction/04-after-window-restart.png) shows `EXISTING_DRAFT_SENTINEL` restored automatically; [capture 05](../evidence/draft-lifecycle-correction/05-existing-draft-restored.png) confirms the same after focusing thread 1.
5. Cmd+N returns a blank draft, [capture 06](../evidence/draft-lifecycle-correction/06-new-draft-after-restart.png). The previously unsent pane was not recovered.

This is not an autosave delay, failure to persist all text, or server restart: the existing-thread control was saved and restored in the same sequence. The precise boundary is **window process termination/restart after autosave**, not a tested normal Quit menu action. Models, attachments, multiple new panes, and promotion at first send were not tested.

Root cause is consistent with the observed boundary. `composer_edited` in `crates/ferrite-cockpit/src/cockpit.rs:2218` only calls `keep_draft` for a pane with a `ThreadId`. A new draft has no thread yet. `cockpit/drafts.rs:9` sets the one-second write timer and `:28` persists thread-owned text; the new pane has no record in that store. The existing source comparison to T3's persisted draft-session bindings remains in `composer-02`; this run makes no new T3 runtime claim.

Repair contract: draft ownership must exist before the first send. Recover the draft text and its launch context independently of whether a provider thread has been created; promotion should transfer the same draft once.

## 2. Queued Edit cancels the queued item and merges a different draft

**Runtime reproduced; existing `composer-04`.** The actual click changes both pieces of work immediately. Both texts survive, but their separate identities do not.

Valid reproduction on fresh thread 2:

1. Type `BUSY_SENTINEL` and Enter. Fake provider holds the active turn; `thread get 2` reports `attention: working`.
2. Type `QUEUED_SENTINEL` and Enter. The provider receives `isAsync: true` with UUID `00013cb0-0004-4000-8006-983931946a80`, then acknowledges that UUID as queued.
3. Type `UNRELATED_DRAFT` without sending. `thread get 2` reports `queued_prompts: 1` and `draft: true`. [Capture 08](../evidence/draft-lifecycle-correction/08-queue-before-edit-valid.png) shows the queue row and separate composer. [Queue before](../evidence/draft-lifecycle-correction/queue-before-edit.json) contains the UUID and `QUEUED_SENTINEL`; [draft before](../evidence/draft-lifecycle-correction/drafts-before-queue-edit.json) contains exactly `UNRELATED_DRAFT` for thread 2.
4. `window find queued-edit-0` reports the rendered button at x1176, y773, width24, height24. `window click --selector queued-edit-0` clicks its center.
5. The production adapter sends `cancel_async_message` for that exact UUID, which the fixture acknowledges. `thread get 2` now reports `queued_prompts: 0`. [Capture 09](../evidence/draft-lifecycle-correction/09-queue-after-edit.png) shows the removed queue row and the restored queued text in the composer. The exact [saved draft after](../evidence/draft-lifecycle-correction/drafts-after-queue-edit.json) is `QUEUED_SENTINEL\n\nUNRELATED_DRAFT`; [queue after](../evidence/draft-lifecycle-correction/queue-after-edit.json) has no pending item.

The screenshot's visible first line is not used to claim unrelated text was lost: the saved composer proves it was retained and concatenated. No Save or Cancel edit session appeared. This run does not test two queued items, later resubmission order, or cancellation racing with provider consumption; those remain acceptance work.

The first attempt on thread 1 lacked the native queue capability handshake. Ferrite refused it with “Claude has not announced native queue lifecycle support,” retained its text, and drew no queue row. That is a **fixture limitation**, excluded from this finding; capture 07 and command lines 17–27 are retained so the false start is visible. The valid fresh-thread run is lines 28–40.

Root cause: `crates/ferrite-core/src/engine.rs:3886` requests cancellation with `Restore { prepend: true }`. The recovered prompt path at `crates/ferrite-cockpit/src/cockpit.rs:2339` explicitly combines held and current text using two newlines at `:2347`, then replaces the composer. This matches the observed UUID cancellation and saved result. T3's separate queued-edit target and Save/Cancel comparison is already sourced in `composer-04`.

Repair contract: preserve the queued message and unrelated draft as separate objects until the operator commits an edit, or explicitly present this operation as Take back and explain its effect. A pencil/Edit control should not silently turn two independent instructions into one future message.

## 3. Reopen closed surface returns a blank SideChat tab

**Runtime reproduced.** SideChat initially opens with content and an editable composer. Closing its tab destroys the resource. The shared reopen action returns the tab without rebuilding it.

Reproduction on fresh thread 3:

1. Type `Produce fixture text` and Enter. Fake provider returns a fixed reply. Double-click the visible word at logical x525,y163. After the selection toolbar settles, `window find selection-side-chat` succeeds. Click that rendered control.
2. [Capture 13](../evidence/draft-lifecycle-correction/13-sidechat-open.png) shows the SideChat tab, header, conversation, and composer with the selected passage. `window find side-chat-composer` reports x1028,y785,width404,height106. `thread list` includes a new child thread 4.
3. Type `SIDECHAT_DRAFT_SENTINEL`. [Capture 14](../evidence/draft-lifecycle-correction/14-sidechat-draft-before-close.png) shows it in the side composer. A subsequent explicit click into `side-chat-composer` and extra typed sentinel confirms input routing in [capture 15](../evidence/draft-lifecycle-correction/15-sidechat-draft-confirmed.png). No side-chat prompt was submitted.
4. `window click --selector panel-tab-SideChat-0-close`. The tab closes, leaving Overview, [capture 16](../evidence/draft-lifecycle-correction/16-sidechat-closed.png).
5. Cmd+K, type `Reopen closed surface`, inspect the real palette result ([capture 17](../evidence/draft-lifecycle-correction/17-sidechat-reopen-palette.png)), then Enter.
6. `window find panel-tab-SideChat-0` succeeds. The panel body is blank in [capture 18](../evidence/draft-lifecycle-correction/18-sidechat-reopened.png). Clicking the restored tab again and capturing a settled frame produces the same blank result ([capture 19](../evidence/draft-lifecycle-correction/19-sidechat-reopened-settled.png)). Finds for `side-chat`, `side-chat-head`, and `side-chat-composer` all fail with “nothing drawn.” `thread list` now contains only 1,2,3: child 4 was removed.

This is stronger than a missing text restore: the reopened surface has no header, conversation, or input control at all. The initial working view is the control. The second settled capture and explicit tab activation rule out merely observing Overview or a transient frame. The quote's reply was fixture content; this does not establish real-provider fork continuity or a completed side-chat conversation. Cmd+W targeting was not exercised.

Root cause: `cockpit/panel.rs:290` saves the close then calls `release_surface`; its SideChat branch at `:317` calls `close_side_tab`. `cockpit/side_chat.rs:308` removes the tab object, drops its composer, and retires its child thread. `cockpit/palette.rs:1296` later calls only `reopen_closed()`. `side_chat_body` at `cockpit/side_chat.rs:607` returns an empty div when the backing tab is absent. The tab identity is restored; its backing resource is not.

Repair contract: ordinary close/reopen must preserve or reconstruct the conversation and draft. If a surface is intentionally destructive, the UI must not offer a reopen action that creates a dead tab. Keep deletion distinct from closing a view.

## Executable evidence checks and limits

These single-command checks assert the expected contracts **against the retained runtime evidence**. They do not relaunch Ferrite or claim a fresh replay. Each checks a working precondition before failing on the observed defect. Raw results are [contract-results.json](../evidence/draft-lifecycle-correction/contract-results.json).

```sh
python3 docs/audits/2026-11-13-t3-parity/evidence/draft-lifecycle-correction/assert-contract.py new-draft
python3 docs/audits/2026-11-13-t3-parity/evidence/draft-lifecycle-correction/assert-contract.py queue-edit
python3 docs/audits/2026-11-13-t3-parity/evidence/draft-lifecycle-correction/assert-contract.py sidechat-reopen
```

Each returned exit 1 with the corresponding failed contract. Exact live reproduction inputs are above and in `commands.jsonl`; the owned fixture is stopped. Do not rerun its commands against an unrelated running Store. The driver's `prepare` refuses to overwrite an existing state file.

Headless screenshots are settled endpoints, not motion evidence. Some captures contain partial background repaint artifacts; they are not classified as product visual defects. The retained text, actual input responses, persisted state, provider UUID exchange, and rendered-element presence/absence support these three workflow findings.

## Cleanup

All still-open scratch threads (1,2,3) were explicitly closed; SideChat close had already removed child 4. `thread list` returned `[]`. All seven tracked process IDs—server, original/restarted headless windows, and four fake-provider processes—and all three owned process groups were verified absent. [cleanup.json](../evidence/draft-lifecycle-correction/cleanup.json) contains commands and checks. No background model work remains from this investigation.
