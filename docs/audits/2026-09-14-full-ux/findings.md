# Ferrite UX audit — detailed findings

Audited source: `fbc03f3` on `audit/full-ux-2026-09-14`. Read the [summary and verification boundaries](README.md) first. Findings are ordered by recommended triage priority. P1 means major task/control difficulty; P2 means a recoverable problem or missing inspection capability. No product fixes were made.

Each entry retains its specialist’s concrete scenario, evidence, impact, and proposed remedy. “Source-verified” is not a claim of native runtime reproduction. Lead-verification notes take precedence over an earlier specialist caveat. Source line numbers refer to the audited commit.

<a id="ux-01"></a>

## UX-01 · P1 — Settings sends or parks the hidden Thread

**Evidence class:** R. **Original review:** SP-02.

- **Scenario/repro:** In a demo Thread, type an unsent draft; open Settings with Cmd/Ctrl+, and press Enter immediately, before focusing a field. Separately, press Cmd/Ctrl+W while Settings is open.
- **Current behavior:** Opening Settings focuses its enclosing card, not a control. The card blocks pointer propagation but does not intercept cockpit actions. Enter maps globally to `Submit`; that handler has Project-editor and rename guards but no Settings guard, then reads/takes the focused Pane's Composer. `CloseThread` similarly has no modal guard and closes the underlying Pane. Settings remains open.
- **Impact:** A user trying to confirm or close Settings can send a draft or park a Thread they cannot currently see. This is an unintended operational action, not a cosmetic focus issue.
- **Evidence:** `crates/ferrite/src/keymap.rs:152` (unscoped Enter), `:203` (Close Pane); `crates/ferrite/src/cockpit.rs:2621`–`:2636` (card only tracks focus / blocks mouse), `:6440` (focus card), `:3128`–`:3156` (Submit guards and reads underlying Composer), `:5758`–`:5762` (unguarded Close Pane), `:6533` and `:6547` (actions mounted on common root).
- **Recommendation:** Give Settings its own action boundary. Enter must only activate a focused Settings control; Cmd/Ctrl+W should close Settings; cockpit-mutating actions should be suppressed while the modal is up. Trap Tab within its controls rather than allowing background traversal then repairing focus on render.
- **Specialist confidence:** High; lead follow-up reproduction below supersedes the initial source-only limitation.

### Lead verification

Two temporary GPUI tests reproduced both actions: Enter sent `["Draft I have not decided to send"]`; Cmd+W left `settings_open=true, visible_threads=[]`. Neither action was attempted in the real user session.

[Back to priority index](README.md#priority-index)

<a id="ux-02"></a>

## UX-02 · P1 — Typing a sentence can authorize a tool on its first letter

**Evidence class:** S. **Original review:** D3.

**Scenario/repro:** With a normal tool Approval pending at L1 and an empty Composer, follow its `Reply to the Decision…` placeholder and start typing `yes, but first show me the diff`. The first `y` immediately allows the tool. Starting `no, use the other approach` denies on the first `n`. Where standing permission is offered, starting `actually…` can invoke Always on the first `a`.

**Behavior:** Ordinary unmodified letters inside the focused text editor are treated as decision actions only while the editor is empty. Typing has no Enter/Send boundary before permission is sent. The exception for free-text Questions demonstrates the intended safeguard exists only for that request type.

**Impact:** The interface explicitly invites a written reply and then treats its initial character as authorization. Conditions, corrections, and explanation are lost at the exact point users are trying to control an agent's action. This is an actual interaction hazard, not a text-size edge case.

**Sources:** `crates/ferrite/src/pane.rs:2888` (placeholder); `crates/ferrite/src/pane.rs:2647` (Composer's Decision key context); `crates/ferrite/src/keymap.rs:158` (unmodified y/n/a); `crates/ferrite/src/cockpit.rs:3493` and `:3510` (empty-input shortcut path).

**False-positive check:** This is intentional current behavior, already asserted by `a_pending_decision_keeps_the_composer_live_and_an_empty_line_answers` at `crates/ferrite/src/cockpit.rs:15753`. It is a UX design defect rather than a regression against that test. The test even avoids initial y/n/a when entering its example sentence.

**Recommendation:** Keep unmodified letters as text while focus is in the Composer. Put quick permission keys on an explicitly focused Decision region or use a modifier; require deliberate action before sending an authorization. Align the placeholder with what sending a typed message actually does.

**Confidence/evidence:** High, source and existing end-to-end behavior contract; not separately reprobed because its existing test explicitly asserts the behavior.

[Back to priority index](README.md#priority-index)

<a id="ux-03"></a>

## UX-03 · P1 — Required MCP form input goes into the Composer

**Evidence class:** R. **Original review:** D2.

**Scenario/repro:** A single Main request asks for a required integer `Count` between 1 and 3 with no default. No Subagents or other Decisions are present. Click inside the bottom input of the form field, type `2`, then click Send.

**Observed:** The field cannot retain input focus. Nothing is submitted; the Composer contains `2`. The render focus guard explicitly protects Questions, unattributed requests, multiple requests, and Subagents, but omits a single attributed `DecisionKind::Form`. The ordinary render pass consequently refocuses the Composer.

**Impact:** Users cannot complete required MCP elicitation forms in the common single-request case. Typing is diverted into a prompt draft, and a subsequent Enter risks sending the content to the agent instead of the requested form.

**Sources:** `crates/ferrite/src/cockpit.rs:6416` through the omitted Form classification at `:6425`; forced refocus at `:6456`; form inputs are rendered at `crates/ferrite/src/cockpit/subagents.rs:1014`.

**False-positive check:** The existing `contract_mcp_required_input_without_default_is_editable_and_validated` only types an invalid value and checks that submission is rejected. That also passes when the required field stayed empty. The audit probe uses a valid value and asserts actual transport. It was repeated with a click specifically in the lower input area, not the section's label/description center. Output stayed `got []; composer=2`.

**Recommendation:** Give every request island the same native-control focus ownership, including Form/External/Approval controls. Verify valid entered data reaches the reply, and that a form keeps focus through provider/UI repaints.

**Confidence/evidence:** High, runtime reproduced. Probe `audit_mcp_can_enter_valid_value`.

[Back to priority index](README.md#priority-index)

<a id="ux-04"></a>

## UX-04 · P1 — An approval breaks keyboard control in a normal four-pane grid

**Evidence class:** R. **Original review:** D1.

**Scenario/repro:** Open four Threads as a Group at 860×500 (the project's existing normal Instruments/L2 test size). Leave focus in the first Pane. Have its Session request a simple approval. Press the displayed `y allow`, then Cmd+F to enlarge and inspect it.

**Observed:** Neither key works. Headless reproduction returned `y answered=false; cmd-f fullscreen=false`. The L2 rendering path removes the Composer when a Decision is present, while focus routing still insists on focusing that unmounted Composer. The visible Decision has its own focus target, but the earlier exhaustive L1/L2 branch prevents reaching it.

**Impact:** Exactly when a dense cockpit needs the user, its keyboard workflow stops. The advertised approval key fails; the normal fullscreen escape route fails too. Mouse approval or moving focus elsewhere can recover, but users cannot trust keyboard supervision.

**Sources:** `crates/ferrite/src/pane.rs:1517` (Decision early return omits Composer); `crates/ferrite/src/cockpit.rs:6390` (L2 always focuses Composer); `crates/ferrite/src/cockpit.rs:6393` (Decision target is shadowed by prior match).

**Recommendation:** Resolve focus from the controls actually mounted in each Pane. When L2 replaces its Composer with a Decision, focus the Decision target and preserve global shortcuts. Alternatively keep the Composer mounted consistently if that is the intended L2 workflow.

**Confidence/evidence:** High, runtime reproduced with production GPUI key dispatch. Probe `audit_l2_approval_keeps_advertised_keys_working`.

[Back to priority index](README.md#priority-index)

<a id="ux-05"></a>

## UX-05 · P1 — The advertised Park shortcut leaves the agent running

**Evidence class:** S. **Original review:** ON-02.

**Scenario / reproduce:** In a Group, open a member's nav context menu, observe **Park Thread ⌘W**, dismiss, and press ⌘W. Compare with clicking **Park Thread**.

**Current behavior:** Clicking the row invokes `MenuVerb::Park`, stops the Session and retains membership. Pressing ⌘W invokes `CloseThread`, which calls core `close_thread`. In a Group this applies `GroupChange::Leave` and returns before `park`; the Session stays hot. A two-member Group dissolves. An existing test explicitly asserts this outcome with “leaving never parks.” The README also calls cmd-W “park a Thread.”

**Impact:** Following the visible shortcut can destroy the operator's Group arrangement and leave an agent consuming resources or continuing work offscreen. Cmd-O does not undo that Group removal because the Thread was never parked.

**Evidence:** `crates/ferrite/src/cockpit.rs:1694`–`:1699` (Park label/shortcut/verb); `:1908`–`:1921` (actual park); `:5756`–`:5762` (keyboard close); `crates/ferrite-core/src/cockpit.rs:3597`–`:3634` (Group leave, early return); `:3638`–`:3644` (Solo park); `crates/ferrite/src/cockpit.rs:9331` (test assertion). Committed README Quickstart shortcut list.

**Recommendation:** Decide one honest primary lifecycle action and align the advertised shortcut, menu item and README. If Close Pane intentionally changes durable membership, label it distinctly and provide a real keyboard Park action; make the continued Session state evident and provide undo for membership removal.

**Confidence:** High; source and existing behavior test agree. This is a UX contract defect even though the core behavior is intentional.

[Back to priority index](README.md#priority-index)

<a id="ux-06"></a>

## UX-06 · P1 — A new Thread can switch the checkout under live Threads

**Evidence class:** E/S. **Original review:** ON-03.

**Scenario / reproduce:** Run Thread A on a Project's main checkout on a feature branch. Start Thread B in the same Project, select **main** or **New branch**, and send. Thread A can still be working.

**Current behavior:** First send calls `git switch <branch>` or `git switch -c <generated branch>` directly in the shared Project root. There is no check for other live Threads bound to that root and no warning naming the Threads affected. The only menu context is “existing branch” or “in the project checkout.” Thread A's cwd remains the same path while its branch and potentially file contents change under it. The switch occurs before Session startup, so a subsequent provider startup failure does not undo the checkout change.

**Impact:** A normal parallel-agent action changes the environment of an already running task. Existing work can continue on the wrong branch or different source files. This is particularly material in a product intended for many simultaneous Sessions.

**Evidence:** `crates/ferrite/src/cockpit.rs:4940`–`:4957` (offered actions/copy); `crates/ferrite-core/src/cockpit.rs:822`–`:845` (unconditional switch before create/start); `crates/ferrite-core/src/workspace/mod.rs:87`–`:95` (exact Git commands). A temporary Git fixture confirmed the same path changed from “feature context” to “master context” immediately after switch.

**Recommendation:** Resolve live Threads sharing the checkout before allowing the move; surface the exact affected Threads and provide an isolated-worktree route. Shared checkout changes should be explicit operations with a clear effect on existing Sessions, not an implicit consequence of sending a new Thread's first prompt.

**Confidence:** High for filesystem/UI behavior; no claim that an actual provider lost work during this audit.

[Back to priority index](README.md#priority-index)

<a id="ux-07"></a>

## UX-07 · P1 — Project navigation silently retargets a populated draft

**Evidence class:** S. **Original review:** ON-04.

**Scenario / reproduce:** Prepare a draft for Project A, choose an isolated worktree, and type a real task. Navigate to an existing Thread so the draft is no longer on screen. Change the nav Project filter to B to inspect its Threads. Return to the draft using New Thread and send its unchanged text.

**Current behavior:** `choose_nav_filter` searches all Panes for a non-starting draft, not just the focused or visible one, and changes its Project. `DraftBinding::choose_project` resets its Workspace target to Main. The prompt remains. Returning through ordinary New Thread additionally reuses the standing draft and overwrites its target with the requested Main target.

**Impact:** Navigation intended to inspect another Project can move an already composed task to a different repository and discard the planned worktree isolation. The visible chip on returning does disclose the new target, but the mutation occurred while that chip was hidden and was not an execution choice.

**Evidence:** `crates/ferrite/src/cockpit.rs:4640`–`:4653`; `crates/ferrite-core/src/draft.rs:88`–`:101`; standing-draft reuse/target overwrite at `crates/ferrite/src/cockpit.rs:4687`–`:4701`. Existing test `choosing_a_project_moves_the_standing_draft` at `:9974` verifies the basic retargeting, but does not protect composed/hidden drafts. Nav filter comment promises navigation-only behavior at `:8395`–`:8401`.

**Recommendation:** Bind a populated draft to its chosen Project/workspace until the operator changes the draft's own controls. Use navigation context only to initialize an empty new draft; retain a standing draft's isolation choice when merely bringing it back into view.

**Confidence:** High; deterministic state transition. No assertion of actual wrong-repository edits in this audit.

[Back to priority index](README.md#priority-index)

<a id="ux-08"></a>

## UX-08 · P1 — Parking loses the unsent prompt and attachments

**Evidence class:** S. **Original review:** C1.

- **Scenario / reproduction:** On an existing Thread, write a follow-up and attach a screenshot. In Solo, press Cmd+W to park it; press Cmd+O to reopen it. This is an ordinary cockpit operation used to manage live Sessions. The old transcript comes back, but the unsent prompt and screenshot are gone. Quitting/relaunching likewise has no persistence for Composer state.
- **Actual behavior / impact:** Unsent work belongs only to `PaneView`'s Composer entity. Close performs no dirty-draft check or save, then drops the view. Reopen creates a blank Composer. This loses composed instructions without an opportunity to recover via history or undo.
- **Evidence:** `crates/ferrite/src/cockpit.rs:5756` describes close as park; `:5787` calls the core close with no Composer check; `:5809` synchronizes panes. `:808` drops removed PaneViews; `:817` constructs fresh ones. `crates/ferrite/src/pane.rs:200` constructs a new empty Composer. `crates/ferrite/src/composer.rs:137` initializes files/text empty.
- **Recommendation:** Preserve unsent text and attachment references per Thread across parking/reopening and app restarts. New unsent draft Panes may require a separate discard policy, but closing a durable Thread should not discard its pending prompt.
- **Confidence:** High, source-confirmed end-to-end state lifetime. No GUI reproduction by this subagent. False-positive check: merely changing Solo/Group focus does **not** drop all other open Panes; `Roster::panes()` holds all open Panes, so normal focus switching is excluded from this finding.

[Back to priority index](README.md#priority-index)

<a id="ux-09"></a>

## UX-09 · P1 — Some refused sends erase the prompt

**Evidence class:** S. **Original review:** C2.

- **Scenario / reproduction:** A Thread's Session has failed to restart (for example after the watchdog), and starting its replacement still fails. Write a substantial retry prompt and submit. Or send after the configured session project root has disappeared. Ferrite shows a failure Notice, but the typed prompt is gone.
- **Actual behavior / impact:** The UI clears Composer before calling the core's void `send`. The early missing-root and spawn-failure exits record only an error. They neither restore the text, queue it, nor append it to history. Cmd+Z cannot recover it because `take` clears the undo stack. The user must reconstruct their prompt after fixing the original failure.
- **Evidence:** `crates/ferrite/src/cockpit.rs:3156` takes/clears Composer; `:3198` calls `send` without a result or restoration. `crates/ferrite/src/composer.rs:246` clears text/files; `crates/ferrite/src/line.rs:116` clears undo/redo. `crates/ferrite-core/src/cockpit.rs:1572` returns on vanished root; `:1599` returns on spawn error. Existing tests at `:6443` and `:6506` explicitly verify these refused prompts never reach prompt history.
- **Recommendation:** Return a send outcome distinguishing accepted, held, and refused. On refusal preserve the original Composer text and attachments, with the error adjacent to it. Preserve edits made during asynchronous sends as well.
- **Confidence:** High, source-confirmed and supported by existing failure tests. No live failure induced. False-positive check: this is **not** every failed send; failures after `deliver` are already held at core `:1614`, and unsupported busy queue submission restores Composer at UI `:3194`.

[Back to priority index](README.md#priority-index)

<a id="ux-10"></a>

## UX-10 · P1 — A custom single-choice answer sends the old radio choice too

**Evidence class:** R. **Original review:** D4.

**Scenario/repro:** In an ordinary single-choice Question, click the first radio option. Change your mind and type `neither, wait 2 days` in `Or write your own answer…`. Click Send answer.

**Observed:** The reply is `Answer { picks: [0], other: Some("neither, wait 2 days") }`. The selected radio is never cleared by entering custom text, and the formatter concatenates picked labels with the custom answer. For Claude this becomes one comma-joined answer; Codex receives both array entries. Choosing a radio after entering custom text has the symmetric stale-text issue.

**Impact:** An everyday change of mind sends contradictory instructions. The user cannot provide only a custom alternative after making a selection because the radio group has no clear-selection action. This can send “Proceed” along with “do not proceed.”

**Sources:** `crates/ferrite/src/cockpit/subagents.rs:1566` (radio stores pick only); `:1584` (independent custom input); `:1701` (submit retains both); `crates/ferrite-core/src/questions.rs:200` (both values combined); `crates/ferrite-core/src/providers/codex/questions.rs:341` (native answer carries all values).

**Recommendation:** Treat custom text as an explicit mutually exclusive Other choice for single-select questions. Choosing Other should clear radio selection; choosing a radio should clear or disable the Other value. Preserve the ability to combine selections and custom text only for multi-select questions.

**Confidence/evidence:** High, runtime reproduced. Probe `audit_custom_single_answer_replaces_choice`.

[Back to priority index](README.md#priority-index)

<a id="ux-11"></a>

## UX-11 · P1 — Earlier transcript history becomes unreachable after 200 blocks

**Evidence class:** R. **Original review:** TR-01.

- **Scenario/repro:** Run a normal multi-turn coding Thread containing more than 200 prompt, reasoning, prose, and tool blocks, then scroll back to its initial instructions or an earlier test result. A block is not a turn: several are generated during ordinary work. For a deterministic fixture, emit 201 separate `Input::Prompt` blocks and inspect the L1 projection.
- **Current behavior:** Core retains 2,000 blocks, but L1 takes only the most recent 200. The virtualized transcript and its logical copy document receive that truncated slice. The top of the scrollbar is consequently the oldest of those 200 blocks, with no older-history loader or indication that earlier content remains outside this view. A newly arriving block can also age a currently reviewed first row out of the view.
- **Impact:** The operator cannot check earlier requirements, decisions, results, or copy context from the same Thread. This is a normal session-history boundary, independent of the length of any individual message. Increasing native list virtualization did not remove the separate 200-block presentation window.
- **Evidence:** `crates/ferrite-core/src/docview.rs:227` (`Level::Transcript => 200` at 230); `crates/ferrite/src/pane.rs:2379` (tail slice); `crates/ferrite/src/cockpit.rs:893` (only that slice enters TranscriptInput); `crates/ferrite-core/src/transcript.rs:487` (2,000 retained blocks); `crates/ferrite/src/transcript.rs:276` (selection document uses only projected rows).
- **Recommendation:** Let the already virtualized L1 list address retained history and lazily load persisted history beyond that. Mark any real history boundary explicitly. Preserve the reader's anchor when loading older records.
- **Specialist confidence:** High; lead follow-up reproduction below supersedes the initial source-only limitation.

### Lead verification

The temporary GPUI test produced `core_blocks=201, core_contains_first=true, ui_contains_first=false, ui_contains_second=true, ui_contains_last=true`. The oldest message was absent from the entire UI selection document, not merely outside the current viewport.

[Back to priority index](README.md#priority-index)

<a id="ux-12"></a>

## UX-12 · P1 — Diff display and copying strip meaningful indentation

**Evidence class:** R. **Original review:** TR-02.

- **Scenario/repro:** Review a Python, YAML, or Makefile change, or a whitespace-only change such as `-    return value` / `+        return value`. Select and copy the hunk's source text.
- **Current behavior:** Every added/removed line strips the diff marker and then calls `trim_start()`. Context lines also call `trim_start()`. Both render and selection receive the stripped result. The two differently indented example lines therefore display the same `return value`; copied text is also unindented.
- **Impact:** Ferrite's review view conceals changes that alter program semantics and produces misleading code when copied. This happens with small ordinary changes, without truncation or extreme content.
- **Evidence:** `crates/ferrite/src/pane.rs:5144`–5148; stripped `body` is passed to `selection.line` at `crates/ferrite/src/pane.rs:5177`. The nearby comment explicitly describes flattening indentation, confirming this is current intended rendering rather than incidental layout loss.
- **Recommendation:** Remove only the unified diff marker; preserve all remaining source bytes for rendering and selection. Make indentation visible through exact whitespace layout, with optional whitespace markers for whitespace-only edits.
- **Specialist confidence:** High; lead follow-up reproduction below supersedes the initial source-only limitation.

### Lead verification

The actual renderer probe registered `["if ready:", "return old_value", "return new_value", "make all"]` from a fixture containing four-space indentation and a Makefile tab. Display/copy sources both lose that whitespace.

[Back to priority index](README.md#priority-index)

<a id="ux-13"></a>

## UX-13 · P1 — Codex multi-file hunks omit filenames

**Evidence class:** S. **Original review:** TR-03.

- **Scenario/repro:** A Codex `fileChange` item modifies two files. Expand its tool group/card and inspect the colored hunks to review which change belongs to which file.
- **Current behavior:** Codex supplies the native item containing `changes[]`. `tool_summary` recognizes string fields such as `path` and `command`, but not the paths nested in `changes[]`, so the call header can read only `fileChange`. `render_diff` never renders `diff.path`; it stacks each file's rows under that generic header. The old assumption that the tool header already names the file does not hold for this provider's multi-file calls.
- **Impact:** The main diff view cannot support reliable file-by-file review. The operator must decode the expanded raw JSON or another view to associate hunks with their source files, particularly dangerous when several files have similar code.
- **Evidence:** `crates/ferrite-core/src/providers/codex/wire.rs:409`–423 (native item as input); `crates/ferrite-core/src/providers/codex/wire.rs:448`–461 (each change's path retained in FileEdit); `crates/ferrite-core/src/transcript.rs:1535`–1558 (summary keys); `crates/ferrite/src/pane.rs:4406` (generic name used for empty summary); `crates/ferrite/src/pane.rs:4605` (iterates diffs); `crates/ferrite/src/pane.rs:5087`–5204 (hunk rendering has no path).
- **Recommendation:** Give each file diff a selectable, openable file header, including operation and path. Do not rely on tool-input summaries to provide the diff's identity.
- **Confidence:** High, verified complete provider-to-renderer path; current native visual verification remains with root.

[Back to priority index](README.md#priority-index)

<a id="ux-14"></a>

## UX-14 · P1 — Provider handover is hidden inside an ordinary model choice

**Evidence class:** V/S. **Original review:** C3.

- **Scenario / reproduction:** After several turns with Claude, open the model chip and choose a Codex model (or the reverse). The menu shows model names/icons, but no explanation that the other provider starts a fresh Session with a digest of previous exchanges.
- **Actual behavior / impact:** This is a material conversation-state transition hidden in a routine model list. The other provider has not actually seen the original conversation. Users cannot evaluate that consequence before choosing, and effort is also reset to the other provider's default.
- **Evidence:** `crates/ferrite/src/cockpit.rs:4441` creates the section detail “hands the conversation over”; `:7941` maps rows into `Choice` without `detail`. `crates/ferrite/src/components.rs:35` has no detail field and `:96` skips section rows entirely. `crates/ferrite-core/src/cockpit.rs:1138` routes an established Thread to `hand_over`; `:1166` replaces the Session with `ReplacementKind::Handover` and `None` effort. CONTEXT.md defines the consequence explicitly.
- **Recommendation:** Show cross-provider choices as a clearly labeled handover group with the fresh-Session/digest consequence before selection. Keep same-provider model changes visually separate. Display the resolved “Default” effort explanation too: the same lost-detail seam strips that information.
- **Confidence:** High, complete source chain. Lead verified the menu appearance in the running app; see below. No claim of transcript deletion: transcript preservation and digest handover are implemented.

### Lead verification

The lead opened the model picker in the running development app. It showed one uninterrupted list of Claude and Codex model names/icons, with no provider section headings or handover explanation. No model was selected or provider switched.

[Back to priority index](README.md#priority-index)

<a id="ux-15"></a>

## UX-15 · P1 — An open model picker can execute a different row after discovery

**Evidence class:** S. **Original review:** C4.

- **Scenario / reproduction:** Open the model picker soon after launch or while another Session is announcing its models. Leave it open until discovery updates/reorders the catalog, then click a visible model row. The menu can retain the old label while executing the new model/provider at its old index.
- **Actual behavior / impact:** Native `ChoiceMenu` builds its PopupMenu only once per opening. Each click captures a numeric row index. Meanwhile the cockpit updates the source `Popover.rows`. The displayed menu and the action lookup can therefore describe different choices; a row can choose another model, possibly another provider, or become inert/out of range.
- **Evidence:** `crates/ferrite/src/components.rs:73–78` invalidates the cached menu only when closed; `:95–110` captures `index` in the callback. `crates/ferrite/src/cockpit.rs:1131–1134` refreshes while discovery changes; `:4993–5020` replaces underlying picker rows. `:7998` calls `pick(at)`; `:4061` resolves that index against the current rows, rather than the row originally rendered. The comment at `:4991` explicitly anticipates discovery finishing while a picker is open. Existing test `:16722` announces before opening, so it does not exercise the mismatch.
- **Recommendation:** Bind each native menu action to a stable provider/model or effort value, and rebuild/update the retained PopupMenu when its choices change. Preserve keyboard selection by value in the actual native menu too.
- **Confidence:** High for code defect; timing scenario source-confirmed, not live reproduced. It requires a changed discovery result while the picker remains open, not normal already-settled catalog usage.

[Back to priority index](README.md#priority-index)

<a id="ux-16"></a>

## UX-16 · P1 — Selecting a file mention with spaces resolves the wrong path

**Evidence class:** E. **Original review:** C6.

- **Scenario / reproduction:** Have `docs/Design Notes.md` or a screenshot such as `images/Screen Shot.png` in the project. Type `@Design`, choose the result, and send. The Composer stages a full-looking mention pill, but the provider parser reads only the path before the first space.
- **Actual behavior / impact:** For images, the selected image is not sent as native image input. For files, the intended mention item is absent or refers to a different real file whose name is the prefix. The agent receives ambiguous raw text and may ask for the attachment again or inspect the wrong file.
- **Evidence:** `crates/ferrite/src/cockpit.rs:4097–4101` inserts raw `@{path} `, without JSON quoting; `:6000` and `:6055` retain the original path from both local and native search rows. `crates/ferrite-core/src/prompt_files.rs:69–71` stops unquoted paths at whitespace. `crates/ferrite-core/src/providers/codex/wire.rs:863–875` resolves those tokens into native items only if the resulting path is a file. Claude uses the same path parser at `providers/claude/wire.rs:20`.
- **Verification:** Compiled and ran the actual `prompt_files.rs` from this worktree. `@docs/Design Notes.md ` resolved to `/tmp/ferrite-example/docs/Design`; `@"docs/Design Notes.md" ` correctly resolved to the full path.
- **Recommendation:** Serialize menu-selected paths with the same quoted token grammar used by attachments. Keep pill display separate from the exact serialized token. Ensure editing or recalling a quoted mention remains usable.
- **Confidence:** High; direct source-module runtime probe. False-positive check: drag/drop and clipboard-file attachments already use the correctly quoted `compose` path, so they are not affected.

[Back to priority index](README.md#priority-index)

<a id="ux-17"></a>

## UX-17 · P1 — Windows Ctrl+A and Ctrl+W perform the wrong edits

**Evidence class:** E/S. **Original review:** C5.

- **Scenario / reproduction:** On Windows, type a prompt, press Ctrl+A, then type its replacement. Ctrl+A moves the caret to the beginning instead of selecting the prompt, so new text is prepended. Press Ctrl+W while Composer holds text: it deletes the preceding word instead of closing the Thread.
- **Actual behavior / impact:** Basic native shortcuts perform unexpected edits on the main work surface. The collision is caused by applying macOS/Emacs Ctrl aliases on Windows, where Ctrl is also the primary application modifier.
- **Evidence:** `crates/ferrite/src/keymap.rs:31–34` chooses Windows Ctrl. `:128` registers Ctrl+A SelectAll; `:142` registers Ctrl+A Home later in the same Composer context. The later same-depth precedence is documented at `:219–230`. `:145` registers Ctrl+W DeleteWordLeft in Composer; `:203` registers the global CloseThread, which loses to the deeper Composer context while text exists. `crates/ferrite/src/composer.rs:498–505` consumes word deletion except on empty text.
- **Verification:** Directly compiled the audited source table in `/tmp/ferrite-composer-audit-probe.rs`. Its output lists both Ctrl+A bindings and both Ctrl+W bindings with the conflicting contexts. No Windows UI session was available.
- **Recommendation:** Make Emacs-style Ctrl aliases macOS-only where they conflict with Windows primary shortcuts. Verify actual dispatch, not merely presence of both bindings in the table.
- **Confidence:** High source confidence; Windows runtime not tested. Do not count Ctrl+Shift+Left/Right as a separate defect: their later selection bindings may correctly win after modifier normalization.

**Independent corroboration:** SP-03 traced actual dependency dispatch ordering: `gpui-pre-0.3.3/src/keymap.rs:173` and `:188` prefer deeper contexts, then later bindings. This is still not a Windows-native runtime test.

[Back to priority index](README.md#priority-index)

<a id="ux-18"></a>

## UX-18 · P1 — New Thread in this Group creates an ungrouped Thread

**Evidence class:** S. **Original review:** ON-01.

**Scenario / reproduce:** Open a Group, right-click its navigation header, select **New Thread in this Group**, then submit a prompt.

**Current behavior:** The menu row dispatches `MenuVerb::NewThread`. Its shared handler looks up the Group's first member's Project, calls `open_draft(DraftTarget::Main, ...)`, and only updates the Project. `open_draft` selects `DraftPlacement::Loose`; the core switches to Solo with an empty Group scope. The new Thread therefore does not join the Group. The keyboard shortcut and titlebar add button do have a separate correct current-Group path, making this an inconsistent action rather than an absent feature.

**Impact:** The explicitly requested grouped workflow is broken. The operator leaves the board they were managing and must reconstruct the intended placement after sending.

**Evidence:** `crates/ferrite/src/cockpit.rs:1755` (label); `:1970`–`:1987` (handler); `:4576`–`:4577` (Loose); `:4763`–`:4766` (placement dispatch); `crates/ferrite-core/src/cockpit.rs:3413`–`:3416` (empty scope and Solo). Existing shortcut contract test: `crates/ferrite/src/cockpit.rs:9337`.

**Recommendation:** Carry the clicked Group ID into the draft's Group scope and show that Group, including when the clicked Group was not the current one. Reuse the current-Group behavior only after explicitly aiming at that ID.

**Confidence:** High; complete UI-to-core call chain verified. No native runtime reproduction by this agent.

[Back to priority index](README.md#priority-index)

<a id="ux-19"></a>

## UX-19 · P1 — New-Thread defaults depend on which creation action you use

**Evidence class:** S. **Original review:** ON-05.

**Scenario / reproduce:** While a Claude Thread is open, set the default Provider to Codex and choose a specific default model; close Settings and press Cmd-T or the nav add button. Separately, park all Threads and relaunch with a non-default model configured.

**Current behavior:** Cmd-T and nav add copy the focused Thread/draft's Provider and model. They consult `default_choice()` only when no focused Pane exists. The startup/first-project draft path uses `open_draft_with_provider`, which explicitly sets `model: None`. The spawner takes `request.model` literally; SessionDefaults includes effort/permissions but no model fallback. The Project-heading add action instead calls `default_choice`, so creation outcomes depend on which add affordance was used. On a fresh installation launched with `--provider codex`, Project creation also uses Settings' default Provider rather than retaining the launch flag.

**Impact:** Operators cannot trust the “What a new Thread starts on” setting. They may repeatedly launch the wrong Provider/model or assume that a chosen cost/capability default is active when it is not.

**Evidence:** `crates/ferrite/src/cockpit.rs:2416`–`:2430` (settings promise); `:4590`–`:4604` (inherits current); `:4617`–`:4632` (model None); `:4662`–`:4671` (other add path uses defaults); `:789`–`:792` and `:2749`–`:2756` (startup and Project creation); `crates/ferrite/src/session.rs:34`–`:43` and `:290`–`:304` (no model fallback). Main parses `--provider` at `crates/ferrite/src/main.rs:98`–`:109`.

**Recommendation:** Apply configured defaults consistently to ordinary New Thread. If “duplicate current Provider/model” is wanted, make it a distinct action or accurately describe that behavior in Settings. Keep the first-run launch Provider and selected model through Project setup.

**Confidence:** High; full choice-to-spawn data flow verified. Coordinate with Settings audit to deduplicate.

**Deduplication:** SP-05 independently found the same startup-model-default bypass and is included here, not counted again.

[Back to priority index](README.md#priority-index)

<a id="ux-20"></a>

## UX-20 · P1 — Changing default model retains unsupported effort invisibly

**Evidence class:** S. **Original review:** SP-04.

- **Scenario/repro:** In Settings → New Threads, select Codex GPT-5.6 Sol and Ultra effort, then switch the model to GPT-5.4 Mini or Luna. Claude Max → Haiku is another example. Open a new Thread using that model.
- **Current behavior:** The model handler changes only `codex_model`/`claude_model`. It preserves the previous effort. Rendering regenerates effort options from the new model's supported ladder, so the previous selection disappears and no effort option is selected. The saved effort still reaches new sessions: the draft labels it, Spawn falls back to Settings, and Codex sends that effort verbatim.
- **Impact:** Normal clicks produce a contradictory configuration: Settings cannot show the active effort, the next draft inherits it anyway, and the provider is asked for a combination its model catalog says is unsupported. This can lead to rejected sends; an actual provider refusal has not been executed by this agent.
- **Evidence:** `crates/ferrite/src/cockpit.rs:2460`–`:2468` (model change does not reconcile effort), `:2482`–`:2490` (renders new ladder only), `:5281`–`:5285` (draft reads stale Settings effort); `crates/ferrite-core/src/settings.rs:150`–`:154` (setter changes only model); `crates/ferrite/src/session.rs:301`–`:304` (unchecked default inheritance); `crates/ferrite-core/src/providers/codex.rs:581`–`:582` (verbatim request effort). Current fallback ladders are at `providers/models.rs:24`–`:26`, `:86`–`:91`, `:95`–`:143`. `draft.rs:62` validates only the draft's explicitly selected effort; it does not validate a Settings default.
- **Recommendation:** Reconcile model and default effort atomically: preserve only supported effort; otherwise select Default and visibly reflect it. Also validate an inherited default against the actual Thread model at first send, since a Thread may override the model.
- **Confidence:** High for invalid saved/displayed/requested combination. Provider refusal remains untested; do not overstate it as an observed runtime failure.

[Back to priority index](README.md#priority-index)

<a id="ux-21"></a>

## UX-21 · P1 — Background process crashes bypass the bell and toast

**Evidence class:** R. **Original review:** D5.

**Scenario/repro:** Keep focus on Thread A while Thread B is working. B's Claude CLI/Codex app-server exits unexpectedly. The adapters emit `SessionEvent::Closed` with the actual exit reason.

**Observed:** Zero new unread Notices. Closing the Session disconnects Activity before the notification observer runs. The observer sees a disconnected Activity and exits without creating a failure record. A recorded `TurnEnded(Error)` would notify, but a process crash takes another path. Even a turn result and process exit folded in the same pump can lose the result's notification.

**Impact:** Ordinary completions attract attention while a crashed, unfinished agent can silently drop out of the supervision workflow. The transcript retains an exit reason and the navigation status becomes blocked, so this is not hidden from every surface; it is missing from the bell/toast surface users rely on across multiple Threads and Project filters.

**Sources:** `crates/ferrite-core/src/cockpit.rs:2382` through `:2400`; `crates/ferrite-core/src/notifications.rs:152`; adapter exit events at `crates/ferrite-core/src/providers/codex.rs:1242` and `crates/ferrite-core/src/providers/claude.rs:937`.

**Recommendation:** Distinguish expected operator parking/interrupt from an unexpected process exit. Create an actionable failure Notice for unexpected exits, retaining the exit reason and a clear way to retry/resume. Preserve completion accounting when a result and exit are drained in one frame.

**Confidence/evidence:** High, runtime reproduced through the real Cockpit pump. Probe `audit_crashed_background_thread_notifies_operator`.

[Back to priority index](README.md#priority-index)

<a id="ux-22"></a>

## UX-22 · P1 — Thread overview hides subagent failures and remaining work

**Evidence class:** S. **Original review:** TR-04.

- **Scenario/repro:** Start five subagents from one Thread in a Group pane. Let the early subagents complete and a later child fail or continue working while Main stays selected.
- **Current behavior:** Visible tabs distinguish only working dots and an actual pending-Decision dot. Failed, complete, idle, interrupted and unavailable states have the same name-only visible tab; their status is in the tooltip. The overflow trigger always reads `+N`, with no working/failure information. Visible tabs are the first discovered children, so completed early children retain scarce slots while newer active children remain in overflow. The separate attention control is driven by pending Decisions, not failures.
- **Impact:** A cockpit meant for parallel supervision requires hovering/opening menus to discover a failed delegation or work hidden behind overflow. The operator can mistake the end of an animation for successful completion.
- **Evidence:** `crates/ferrite/src/cockpit/subagents.rs:333`–369 (width allocation and prefix selection); `crates/ferrite/src/cockpit/subagents.rs:407`–445 (only working/waiting visible state); `crates/ferrite/src/cockpit/subagents.rs:467`–499 (status inside menu, count-only trigger); `crates/ferrite/src/cockpit/subagents.rs:650`–658 (attention returns none absent pending Decisions). `crates/ferrite-core/src/activity.rs:1893` uses discovery order.
- **Recommendation:** Keep an always-visible failed/working/needs-input summary for the Thread's subagents, including hidden ones, and visible outcome marks on each tab. Preserve tab positions; an activity summary avoids disruptive reordering while directing attention to the right child.
- **Confidence:** High, source-verified state rendering. No claim that child completion should generate Ferrite Notices; CONTEXT deliberately reserves those for Main finishing for good.

### Related overview mismatch (merged D6)

**Scenario:** Claude Main has finished its foreground turn while a background Subagent continues working, a lifecycle explicitly supported by the notification deferral logic. Keep Main selected and view the Thread in the navigation or a small L2/L3 Pane.

**Behavior:** Navigation classifies Working from Main's transcript alone. It does aggregate all Decisions, but it does not aggregate child activity. A completed Main therefore produces Idle in navigation and Done in the small Main Pane while Subagents are still running; the Subject tabs that explain that distinction exist only at L1. The child count conveys count, not whether the children are still working.

**Impact:** Users scanning the cockpit can mistake an unfinished Thread for a completed one and open it to find out whether work remains. This undermines the dense grid's main purpose. No premature completion Notice is sent — that part is correct — which makes the mismatch specifically a status presentation issue.

**Sources:** `crates/ferrite/src/cockpit.rs:5593` (Thread nav derives status from Main transcript); `crates/ferrite/src/cockpit.rs:5606` (Done maps Idle); `crates/ferrite/src/pane.rs:846` (selected transcript wall status); `crates/ferrite/src/cockpit.rs:6937` (Subject strip only L1); `crates/ferrite-core/src/notifications.rs:161` (completion correctly waits for live children).

**Recommendation:** Keep Subject status local in the transcript, but show an aggregate Thread status in navigation/overview, e.g. Main idle · 1 agent working. Avoid “done” for the overview while the Thread still owes live work.

**Confidence/evidence:** High code confidence, not runtime screenshot verified; this is a product presentation decision, not an incorrect Subject state.

[Back to priority index](README.md#priority-index)

<a id="ux-23"></a>

## UX-23 · P1 — macOS red-close has no window reopen path

**Evidence class:** S. **Original review:** SP-01.

- **Scenario/repro:** Launch Ferrite on macOS, click the red traffic-light close button, then click its Dock icon (or use its File → New Thread command).
- **Current behavior:** Ferrite creates its sole window only in the initial application `run` callback. It does not register `on_reopen`, a new-window action, or `LastWindowClosed` quit behavior. GPUI's default keeps a macOS app running after its last window closes. The cockpit actions live on the now-dropped view, so New Thread does not construct a new window.
- **Impact:** The user loses access to the app's interface after an ordinary native-window action. Recovery requires quitting the still-running app and launching again.
- **Evidence:** `crates/ferrite/src/main.rs:125` (single initial application lifecycle), `:214` (only window construction), `:237` (view constructed inside that window), `:411` (File menu offers New Thread, no window construction). Dependency `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-pre-0.3.3/src/app.rs:1912` explicitly makes `QuitMode::Default` quit on last-window-close only outside macOS. Whole-app search found no `on_reopen`, `set_quit_mode`, `with_quit_mode`, or other `open_window` production path.
- **Recommendation:** Preserve/recreate the cockpit window on the native reopen event, or explicitly quit on last-window-close if that is the intended product behavior. Ensure restored sessions and unsent drafts follow the chosen close semantics.
- **Confidence:** High, source plus platform dependency. Native close/reopen was deliberately not attempted because the active audit conversation itself runs in this Ferrite instance. This finding remains source/dependency verified.

[Back to priority index](README.md#priority-index)

<a id="ux-24"></a>

## UX-24 · P2 — Multiline line-boundary editing acts on the whole prompt

**Evidence class:** E. **Original review:** C7.

- **Scenario / reproduction:** Paste three separate instructions. Put the caret in the second line and press Cmd+Left or Cmd+Backspace to move to or clear the beginning of that line. The first jumps to the beginning of the entire prompt; the second deletes the entire first instruction as well as the second-line prefix. Home/End on Windows similarly use whole-prompt boundaries. Shift+Up/Down is not bound for extending selection between rows.
- **Actual behavior / impact:** A common prompt-refinement gesture unexpectedly changes unrelated preceding instructions. Undo exists, but the user must notice the damage before sending, then work around the field with pointer selection or repeated character movements.
- **Evidence:** `crates/ferrite/src/keymap.rs:79–91` maps line-edge keys to these operations. `crates/ferrite/src/line.rs:219–240` deletes/moves to offsets `0` and `content.len()`, rather than a hard or visual line boundary. `:291–296` gives shifted line-edge selection the same whole-prompt behavior. Composer has Up/Down but no SelectUp/SelectDown actions or dispatch.
- **Verification:** Ran actual `Line` code with `Keep instruction one\nRevise instruction two\nKeep instruction three`. Cmd+Backspace equivalent after “Revise” yielded ` instruction two\nKeep instruction three`, deleting the preceding instruction too.
- **Recommendation:** Distinguish visual/hard-line boundary actions from whole-document actions, and add vertical selection. Preserve platform conventions for Cmd/Option and Home/End.
- **Confidence:** High, direct source-module probe. Undo successfully exists; this is a recoverable editing defect, not permanent deletion.

[Back to priority index](README.md#priority-index)

<a id="ux-25"></a>

## UX-25 · P2 — Interrupt can immediately continue queued work

**Evidence class:** S. **Original review:** C8.

- **Scenario / reproduction:** Queue follow-up work while a Thread is running, then press the displayed “esc interrupt” because the current approach is wrong and you want to stop and rethink. Queued instructions survive and are run after the interrupt; for Codex, Ferrite explicitly resends them.
- **Actual behavior / impact:** The advertised interrupt is not a reliable way to bring the Thread to rest when queued work exists. The operator must remove pending work separately, one latest item at a time, before interrupting. Nothing in the hint explains that the next work will still run. This is an interaction-design issue even though the behavior is intentional internally.
- **Evidence:** `crates/ferrite/src/pane.rs:2766–2767` displays only “esc interrupt”; queued rows at `:3058` advertise only Backspace unqueue. `crates/ferrite-core/src/cockpit.rs:1653–1657` explicitly implements “stop, then do these”, retaining Claude's queue and arming Codex resend. UI interrupt reaches this directly at `crates/ferrite/src/cockpit.rs:3275–3277`.
- **Recommendation:** Make the hint contextual (“interrupt and run queued”) and provide a clear stop-all/pause-queue action that lets users reach a stable state without deleting each prompt. Preserve queued text for later review.
- **Confidence:** High source confidence; not exercised with live provider. Deliberate current semantics, not reported as a backend implementation error.

[Back to priority index](README.md#priority-index)

<a id="ux-26"></a>

## UX-26 · P2 — Workspace choices invent main and omit real alternatives

**Evidence class:** E/S. **Original review:** ON-06.

**Scenario / reproduce:** Register a normal Git repository whose default branch is `master` or `develop`, with no local `main`. Open the workspace picker, choose the offered **main — existing branch**, then submit a prompt. A related everyday case is wanting another Thread on a previously created existing branch/worktree.

**Current behavior:** Any current-branch string other than literal `main` causes a `main` row to be added, without asking Git whether it exists. First send executes `git switch main`, failing with `fatal: invalid reference: main` on a master-only repo. The picker does not list real other branches or existing worktrees although the core has APIs and target variants for them. Non-Git directories are accepted as Projects but still get branch/worktree actions.

**Impact:** A presented existing choice is invalid in a commonplace repo configuration; the operator discovers that only after composing and sending. Existing work cannot be targeted through the chooser without an external branch switch or a separate registration workaround.

**Evidence:** `crates/ferrite/src/cockpit.rs:119` (literal constant); `:4809`–`:4823` (non-Git fallback main); `:4927`–`:4965` (complete workspace rows); `crates/ferrite-core/src/cockpit.rs:832`–`:836` and `crates/ferrite-core/src/workspace/mod.rs:89`–`:95`; real branch API at `workspace/mod.rs:601`–`:609`; existing worktree resolution at `crates/ferrite-core/src/draft.rs:133`–`:155`.

**Probe:** A disposable `git init --initial-branch=master` fixture with one commit returned exit 128 and `fatal: invalid reference: main` for the exact switch command. Fixture: `/var/folders/5y/hnh41b2j29z8kqxy7ftzfdn80000gn/T/ferrite-ux-branch-probe-4dx8uss1`.

**Recommendation:** Build the choices from actual branch/worktree inventory and repository capabilities; do not invent `main` or show Git-only actions for non-Git folders. Preserve the actual current checkout label.

**Confidence:** High; source plus exact Git command probe. UI error display is source-verified and keeps the draft text, which is a positive recovery behavior.

[Back to priority index](README.md#priority-index)

<a id="ux-27"></a>

## UX-27 · P2 — New Thread can use a different Project from the visible Thread

**Evidence class:** S. **Original review:** ON-09.

**Scenario / reproduce:** Work in Project A, then under All Projects open an existing solo Thread in B. Press Cmd-T or the nav add button.

**Current behavior:** Solo draft project resolution chooses `launch_project` instead of the focused Thread's Project. Focusing a Thread does not update that fallback. Group creation and the titlebar “New Group with New Thread” path instead resolve from focused/current Thread context. Thus the UI currently showing B and the draft opened from it can disagree.

**Impact:** Operators must correct the Project chip on routine new-Thread creation and risk sending a task to their previous repository. This is independent of changing the nav filter: it happens under All Projects during normal navigation.

**Evidence:** `crates/ferrite/src/cockpit.rs:4728`–`:4749`; `:5666`–`:5673` (focus only); `:5075`–`:5079` (fallback changes on explicit Project chip pick); `:6664`–`:6680` (titlebar shows focused Project); `:4662`–`:4671` (Project-specific add behaves differently).

**Recommendation:** In All Projects, initialize a new Solo draft from the focused Thread's Project; use last-worked fallback only when no current Project context exists. Apply one rule across add affordances.

**Confidence:** High; deterministic source behavior.

[Back to priority index](README.md#priority-index)

<a id="ux-28"></a>

## UX-28 · P2 — Closing Project editor saves some edits and discards others

**Evidence class:** S. **Original review:** ON-07.

**Scenario / reproduce:** Edit an existing Project, change its name, add/remove an additional directory, then click ×, press Escape, or click the backdrop instead of Done.

**Current behavior:** Directory additions/removals are immediately persisted to the registry; name text is only committed in `confirm_project_editor`. Every close route drops the editor. There is no visible distinction between immediate directory changes and staged name edits, and no cancellation summary.

**Impact:** The same editing session partially saves. Users who close to cancel keep directory changes; users who treat “Done” as dismissal lose the rename. Recovering requires reopening and comparing their intent against the Project state.

**Evidence:** `crates/ferrite/src/cockpit.rs:2716`–`:2734` (name commit); `:2804`–`:2810` (add immediate); `:2908`–`:2918` (remove immediate); `:2875`–`:2879` and `:3009`–`:3014` (close/backdrop discard); `:3255` (Escape takes editor). Existing test documents write-through at `:9846`.

**Recommendation:** Use one clear model: stage all edits until Save with Cancel, or autosave all editable fields and label dismissal Done. Avoid silently mixing them.

**Confidence:** High; source and existing editor test verify behavior.

[Back to priority index](README.md#priority-index)

<a id="ux-29"></a>

## UX-29 · P2 — Project-directory edits leave active Sessions on the old scope

**Evidence class:** S. **Original review:** ON-08.

**Scenario / reproduce:** Start a Thread, open Edit Project, add a sibling directory needed for the next task, press Done, and tell that same Thread to work there. Or remove an additional directory and assume it is no longer exposed to the running Thread.

**Current behavior:** Project directory mutations update the registry only. Additional directories are copied into SpawnRequest when a Session starts. Claude uses startup `--add-dir`; Codex stores its own additional-directories vector and uses that for writable roots. The active Sessions are neither updated nor restarted, and the editor gives no “applies to new/reopened Sessions” indication.

**Impact:** The Project shown in the UI and the current agent's configured directory scope disagree. Users can hit avoidable access prompts/refusals after an apparently successful edit; removing a directory also does not retract it from the active Session's configuration. Actual access remains subject to the Provider's permission/sandbox mode, so a universal denial is not claimed.

**Evidence:** `crates/ferrite-core/src/cockpit.rs:663`–`:681` (registry-only methods); `:65`–`:78`, `:1451`–`:1462` (startup snapshot); `crates/ferrite/src/session.rs:293`–`:331`; `crates/ferrite-core/src/providers/claude.rs:282`–`:283`; `crates/ferrite-core/src/providers/codex.rs:369`, `:798`–`:803`; `crates/ferrite/src/cockpit.rs:2882`–`:2885` (editor instructions contain no application timing).

**Recommendation:** State the effective timing in the editor and identify existing Threads affected; provide a deliberate apply/reopen path, or implement supported live root updates.

**Confidence:** High for configuration lifetime; provider permission outcome varies and was not exercised.

[Back to priority index](README.md#priority-index)

<a id="ux-30"></a>

## UX-30 · P2 — There is no transcript Find; Cmd/Ctrl+F changes layout

**Evidence class:** S. **Original review:** TR-05.

- **Scenario/repro:** Return to an existing Thread and search for a filename, a test name, or an instruction quoted earlier. Press the standard Cmd/Ctrl+F shortcut.
- **Current behavior:** Cmd/Ctrl+F toggles fullscreen. There is no transcript find action, search input, match navigation, or search consumer in the transcript code. Settings and file/session menus have separate search, which does not search transcript content.
- **Impact:** Even before the 200-block limit, reviewing prior work requires manual scrolling and opening collapsed tools. Standard find muscle memory changes the cockpit view rather than helping retrieve context.
- **Evidence:** `crates/ferrite/src/keymap.rs:180`–183; complete transcript surface `crates/ferrite/src/transcript.rs:515`–595 and actions in `crates/ferrite/src/cockpit.rs` contain no transcript-find path. Repository search for Find/Search confirms only unrelated settings/file/session search.
- **Recommendation:** Add per-Subject transcript find with next/previous results and clear hidden-tool match handling. Reserve the platform find chord for it and give fullscreen a distinct discoverable shortcut.
- **Confidence:** High, feature absence plus explicit current shortcut binding; no native reproduction claimed.

[Back to priority index](README.md#priority-index)

<a id="ux-31"></a>

## UX-31 · P2 — Expanding a tool at the live tail moves its header out of view

**Evidence class:** S. **Original review:** TR-06.

- **Scenario/repro:** At the bottom of a Thread, expand a command with an ordinary 20–30 lines of output or expand a multi-tool group in a compact Group pane. Attempt to read the beginning of the revealed input/output or collapse the same header again.
- **Current behavior:** The disclosure click toggles state without suspending tail-follow. Rendering then explicitly scrolls to the bottom whenever follow is still active. The newly grown content can push the clicked header and initial details above the viewport. An existing test documents the behavior and uses a programmatic pause-and-scroll-to-top to reach the header again.
- **Impact:** An explicit reading action unexpectedly relocates the reader. The user must scroll upward to rediscover the control/content they asked to inspect, especially in many-pane layouts.
- **Evidence:** `crates/ferrite/src/transcript.rs:492`–499; `crates/ferrite/src/transcript.rs:522`–524; `crates/ferrite/src/transcript/scroll.rs:121`–123; existing test commentary and workaround at `crates/ferrite/src/cockpit.rs:13470`–13484. The test was read, not rerun.
- **Recommendation:** Anchor the activated disclosure during expansion and treat explicit inspection as temporarily leaving live-follow. Provide a visible Return to latest control to resume. Currently `scroll_transcript_to_bottom` is called for sending/bootstrap/resend, not exposed as a dedicated reader control.
- **Confidence:** High source/test corroboration. Severity could rise if root's native reproduction shows routine clicks consistently conceal the revealed information.

[Back to priority index](README.md#priority-index)

<a id="ux-32"></a>

## UX-32 · P2 — The structured plan cannot be opened for inspection

**Evidence class:** S. **Original review:** TR-07.

- **Scenario/repro:** A Codex Thread reports a four-step plan. While another pane is running, inspect which steps are pending, whether tests are included, or why the agent revised its plan.
- **Current behavior:** Core stores every plan step, status and explanation, but the app consumes only `todos()` and `current_task()` for a noninteractive meter strip. Completed and upcoming step text and the explanation are not exposed in a plan popover or transcript record. A separately authored prose plan can help, but the provider's structured plan itself is not available for inspection.
- **Impact:** The operator can see `2/4` without knowing what the four commitments are, weakening the usefulness of the central progress indicator for steering parallel work.
- **Evidence:** `crates/ferrite-core/src/providers/codex/wire.rs:284`–303; `crates/ferrite-core/src/progress.rs:154`–166; `crates/ferrite-core/src/transcript.rs:701`–725; `crates/ferrite/src/pane.rs:960`–962; `crates/ferrite/src/pane.rs:2237`–2288. Repository search finds no app read of `progress.plan` or explanation.
- **Recommendation:** Make the existing meter open the complete checklist with step statuses and the latest explanation; retain enough plan history to understand material changes.
- **Confidence:** High, source-verified feature gap; distinguish structured progress from approval Decisions and ordinary Markdown prose plans.

[Back to priority index](README.md#priority-index)

<a id="ux-33"></a>

## UX-33 · P2 — Source links discard their line destination

**Evidence class:** S. **Original review:** TR-08.

- **Scenario/repro:** Click an agent's link to `src/lib.rs:240` or `src/lib.rs#L240-L260` during a code review.
- **Current behavior:** The line/column suffix is parsed and stored, but `url()` constructs a URL from `path` only and `open()` opens that URL through the OS default handler. No editor selection or navigation action uses `location`. Existing tests explicitly assert that the suffix is absent from the opened URL.
- **Impact:** The user reaches the file but must manually locate the cited source every time. The central promise of a source reference—taking the reviewer to the evidence—is only partially delivered.
- **Evidence:** `crates/ferrite/src/file_links.rs:58`–73; `crates/ferrite/src/file_links.rs:77`–108; `crates/ferrite/src/cockpit/tests/render_performance.rs:319`–328 (source link test); `crates/ferrite/src/rich.rs:696` (unit fixture).
- **Recommendation:** Offer a configured editor open-at-line action with a safe ordinary-file fallback. Keep the location visible/copyable when line navigation is unavailable.
- **Confidence:** High, explicit implementation and existing test expectation. This is a deliberate current limitation, not an incorrectly encoded file URL bug.

[Back to priority index](README.md#priority-index)

<a id="ux-34"></a>

## UX-34 · P2 — The next prompt removes the prior consolidated Turn changes

**Evidence class:** S. **Original review:** TR-09.

- **Scenario/repro:** Let Codex implement a change, then send a short follow-up such as “run the tests.” Try to return to the implementation turn's consolidated changes for review.
- **Current behavior:** `Input::Prompt` clears the single `turn_diff`. The presentation appends that one optional diff as a tail row, rather than retaining it with its originating turn. On the next prompt the previous row disappears. Individual per-tool file diffs remain, so the code is not necessarily absent from every view.
- **Impact:** The review artifact vanishes during the routine implement → test → review workflow. The user must reconstruct earlier changes from individual tool cards or external Git tools.
- **Evidence:** `crates/ferrite-core/src/transcript.rs:957`–963; `crates/ferrite-core/src/transcript.rs:1088`–1089; `crates/ferrite/src/transcript/rows.rs:240`–246; `crates/ferrite/src/transcript.rs:413`–474.
- **Recommendation:** Retain each completed turn's consolidated diff beside that turn, with its temporal scope clearly labeled. Keep a separate live accumulating diff if needed.
- **Confidence:** High, deterministic state transition and projection; no GUI claim.

[Back to priority index](README.md#priority-index)

<a id="ux-35"></a>

## UX-35 · P2 — Subagent prompt Resend is a visible no-op

**Evidence class:** S. **Original review:** TR-10.

- **Scenario/repro:** Open a Subagent transcript whose saved history includes its delegated prompt. Hover that prompt and click the Resend icon.
- **Current behavior:** All `Body::Prompt` rows get the same Copy and Resend actions. The emitted ResendPrompt event then returns immediately when the Pane's selected Subject is not Main. No disabled state, explanation, or feedback is shown.
- **Impact:** A visible action implies the child can be prompted from this view even though Ferrite deliberately observes provider-managed children. Clicking it gives no result or explanation.
- **Evidence:** `crates/ferrite/src/transcript.rs:389` and `crates/ferrite/src/transcript.rs:392`–410; `crates/ferrite/src/pane.rs:5011`–5025; `crates/ferrite/src/cockpit.rs:971`–974.
- **Recommendation:** Do not render Resend for observed child prompts. Copy remains valid; if a future action transfers a prompt to Main, name that action explicitly and stage it in Main's Composer.
- **Confidence:** High, verified control-to-handler mismatch; scenario requires a child transcript containing a Prompt block, which provider saved-history projections support.

[Back to priority index](README.md#priority-index)

<a id="ux-36"></a>

## UX-36 · P2 — Selecting a child replaces the owning Thread title

**Evidence class:** S. **Original review:** TR-11.

- **Scenario/repro:** In a Group with several Threads from the same repository, open one child Subject in two or more Panes and then return after working elsewhere.
- **Current behavior:** `activity_title` returns the normal Thread title only for Main; for a child, it returns only `agent_name`. The header shows that child name with checkout context, but does not include the owning Thread title or a Thread → child breadcrumb.
- **Impact:** Child names such as Explorer or Reviewer become the main identifiers of several Panes. The operator must consult the nav or return to Main to recover which durable Thread each activity belongs to. This is a context-loss issue even when every name is short.
- **Evidence:** `crates/ferrite/src/cockpit/subagents.rs:617`–639; title wired into every Pane level at `crates/ferrite/src/cockpit.rs:6938`. CONTEXT.md defines Subject switches as changes of view within one Thread, preserving its Title.
- **Recommendation:** Preserve the owning Thread's title and show the selected Subject as a subordinate label or breadcrumb, with full names available on hover.
- **Confidence:** High source verification; many-pane impact should be checked against root's native viewport.

[Back to priority index](README.md#priority-index)

<a id="ux-37"></a>

## UX-37 · P2 — Settings search can show a blank panel for a valid result

**Evidence class:** V/S. **Original review:** SP-06.

- **Scenario/repro:** Open Settings, click About, then search `delete` or `sandbox`. The same occurs from Permissions when filtering down to a single page.
- **Current behavior:** The toolkit filters pages into a shorter list but retains the selected numeric page index (About = 3). The matching result page is now index 0. Rendering only paints a page whose index equals the saved index, leaving the right panel blank even though a matching category exists on the left. Clearing/retyping the query does not reset selection. Clicking the remaining sidebar category recovers it.
- **Impact:** Search appears broken during normal navigation; the user must discover that a matching but unselected category needs another click.
- **Evidence:** `crates/ferrite/src/prefs.rs:125`–`:138` uses toolkit Settings pages. Dependency `gpui-component-0.6.0/src/setting/settings.rs:112`–`:142` filters pages, `:145`–`:163` compares old index to filtered indices, `:380`–`:404` renders without reconciling selection. Existing Ferrite Settings test `cockpit.rs:17158` explicitly returns to page 0 before its next search, so it does not cover this transition.
- **Recommendation:** Keep page identity through filtering and select the first available matching page if the current page is absent; render a clear no-results state only when genuinely empty.
- **Specialist confidence:** High; lead follow-up reproduction below supersedes the initial source-only limitation.

### Lead verification

Native UI reproduction: Settings → About → search `sandbox` left the right panel entirely blank. Clicking the remaining Permissions category immediately revealed the matching sandbox setting. No preference was changed.

[Back to priority index](README.md#priority-index)

<a id="ux-38"></a>

## UX-38 · P2 — Settings search cannot find visible option names

**Evidence class:** V/S. **Original review:** SP-07.

- **Scenario/repro:** Search Settings for `rings`, `full access`, `read only`, or `sonnet`.
- **Current behavior:** These are real choices on screen, but choices are custom-rendered fields and no option labels are passed as search keywords. The toolkit matches only item title, description and explicit keywords. For example, “Usage meter” has a description that never says “Rings”, so searching that exact option removes it.
- **Impact:** Search cannot find settings by the choice the user actually remembers, driving them back to browsing each category.
- **Evidence:** `crates/ferrite/src/prefs.rs:142`–`:174` (custom choices renderer, description only, no keywords); `crates/ferrite/src/cockpit.rs:2542`–`:2554` (Full access / Read only labels), `:2569`–`:2586` (Rings); dependency `gpui-component-0.6.0/src/setting/item.rs:165`–`:178` (matching rules).
- **Recommendation:** Index every visible choice label (plus helpful domain synonyms) in the owning SettingItem's keywords. This is a small indexing fix, not an invitation to add fuzzy search complexity.
- **Specialist confidence:** High; lead follow-up reproduction below supersedes the initial source-only limitation.

### Lead verification

Native UI reproduction: after displaying the `Full access` option, searching `full access` removed every category and left an empty panel. The query was cleared afterward.

[Back to priority index](README.md#priority-index)

<a id="ux-39"></a>

## UX-39 · P2 — Codex model has no route back to CLI default

**Evidence class:** V/S. **Original review:** SP-08.

- **Scenario/repro:** Open Settings → New Threads, select a Codex model, then try to return to the CLI's configured default as the help text describes.
- **Current behavior:** The renderer only maps a catalog entry whose value literally equals `default` to None. Codex's fallback model catalog contains concrete model IDs only; the live `model/list` parser also returns IDs, merely moving `isDefault` to the first position. Ferrite never prepends a distinct Default option. With untouched `codex_model: None`, no chip is selected; once a model is picked there is no UI path back to None. Settings page reset is disabled.
- **Impact:** A reversible preference becomes one-way in the interface and the help text promises an unavailable choice. Users must manually edit settings.json to restore CLI-controlled selection.
- **Evidence:** `crates/ferrite/src/cockpit.rs:2452`–`:2458` (help and literal-default conversion); `crates/ferrite-core/src/providers/models.rs:95`–`:143` (Codex fallback IDs), `crates/ferrite-core/src/providers/codex/wire.rs:801`–`:825` (live IDs/default ordering), `crates/ferrite-core/src/cockpit.rs:2767` (direct catalog); `crates/ferrite/src/prefs.rs:135` (`resettable(false)`).
- **Recommendation:** Always include an explicit “CLI default” choice backed by None, independently of the provider's current default model ID; display the effective default model separately if useful.
- **Confidence:** High, both live and fallback catalogs traced. No model network call required.

### Lead verification

Native inspection showed the Codex model help “Default uses the CLI’s own choice” above concrete model chips only, with no selected chip for the current unset value and no Default chip. The one-way state transition is verified in source, not by altering the user’s preference.

[Back to priority index](README.md#priority-index)

<a id="ux-40"></a>

## UX-40 · P2 — Window size and placement reset at launch

**Evidence class:** S. **Original review:** SP-09.

- **Scenario/repro:** Move Ferrite to another monitor and resize/maximize it for the cockpit; quit and relaunch.
- **Current behavior:** Each launch opens a freshly centered 1440×900 window. There is no window-bounds persistence or restoration path. The `maximized` view field is an in-memory reading used by caption controls, not a saved preference.
- **Impact:** Daily launches discard the operator's desktop arrangement and available cockpit area, forcing them to reconstruct the window before work. No claim is made here that a particular screen clips content; native platform clamping has not been runtime-tested.
- **Evidence:** `crates/ferrite/src/main.rs:213`–`:217`; `crates/ferrite/src/cockpit.rs:769`; `crates/ferrite-core/src/settings.rs:25`–`:64` has no placement data, and whole-app search found no window-bounds observer/store.
- **Recommendation:** Save bounds, display and maximized state on meaningful changes, restore valid geometry, and clamp to currently attached displays.
- **Confidence:** High for reset behavior; exact multi-monitor placement behavior needs native runtime verification.

[Back to priority index](README.md#priority-index)

<a id="ux-41"></a>

## UX-41 · P2 — Events are marked seen while Ferrite is behind another app

**Evidence class:** S. **Original review:** D7.

**Scenario:** Start a turn in the selected Thread and switch to an editor/browser. The Thread finishes or asks a Question while Ferrite is inactive. Return later.

**Behavior:** The core acknowledges the roster's selected Subject on every pump, without any notion of whether the app/window is active. Notices and requests for that Subject are marked read as soon as they arrive, so there is no unread badge or later attention ring for something the user never viewed.

**Impact:** The user loses an accurate “what happened while I was away” marker in the most ordinary multitasking workflow. This does not require adding system notifications: in-app unread state itself is incorrect relative to actual viewing.

**Sources:** `crates/ferrite-core/src/cockpit.rs:2452`, `:3027`, `:3047`; read state mutation in `crates/ferrite-core/src/notifications.rs:347`; `crates/ferrite/src/notifications.rs:178` skips read completion toasts.

**False-positive check:** ADR 0005 explicitly chooses pane-focus acknowledgement and in-app-only silent notifications. Treat this as a current UX tradeoff to revisit; do not report the absence of OS notifications itself as an accidental bug.

**Recommendation:** Let the window report whether the selected Subject is actually visible in the active app before acknowledging new attention. Keep new events unread while Ferrite is inactive; acknowledge when the user returns.

**Confidence/evidence:** High code confidence; inactive native window behavior not separately exercised because parent agent owns GUI.

[Back to priority index](README.md#priority-index)

<a id="ux-42"></a>

## UX-42 · P2 — Settings save failures are hidden behind the modal

**Evidence class:** S. **Original review:** SP-10.

- **Scenario/repro:** With a settings directory that cannot be written, change a preference. Keep Settings open; separately repeat with the main sidebar collapsed.
- **Current behavior:** The in-memory value and session defaults change first. Save failure sets `group_error`. That error is only rendered in the expanded main navigation tree underneath the modal. The collapsed rail does not render it. Settings itself continues showing the selected state without an unsaved marker or retry, and a later successful settings save does not clear this error.
- **Impact:** Users believe a preference was saved until it reverts after relaunch. While collapsed they cannot see the explanation even after closing Settings.
- **Evidence:** `crates/ferrite/src/cockpit.rs:2375`–`:2382` (mutate/save/error/defaults), `:8428`–`:8437` (only error rendering), `:8278`–`:8291` (rail skips nav_tree), `:2621`–`:2651` (modal contains no error status).
- **Recommendation:** Put save feedback in Settings near the changed values; preserve an explicit unsaved state with retry and clear it after successful persistence. Do not report a deferred save as durable.
- **Confidence:** High deterministic error route, but no write-failure fixture executed by this agent. Lower priority than ordinary-path findings above.

[Back to priority index](README.md#priority-index)
