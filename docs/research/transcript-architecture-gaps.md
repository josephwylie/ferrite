# Transcript architecture: gaps against the Claude Desktop handoff

Prepared 2026-11-03 against the `transcript-app-style` worktree (HEAD `21d4810e` plus uncommitted UI work). Source requirements: `docs/research/claude-desktop-transcript-handoff.md` (sections 4–22). This is a read-only audit. Nothing was compiled or run. File:line references are to the working tree as read. Where a claim rests on inference rather than code, it is marked **(guess)**.

Bottom line: Ferrite's core is further along than the handoff assumes. It already has attributed per-subject execution state, delivery-id dedup, completed-block supersession by API message identity, generation-scoped Decisions, and receipt-only todos. The real gaps are narrow:

1. `task_updated` patches are flattened, and a description-only patch clobbers status.
2. There is no `run_id` and no `parent_task_id`.
3. `tool_use_summary` loses its call ids.
4. A session that dies mid-turn leaves no turn outcome.
5. A result that arrives before its call row is dropped.
6. No row exists while tool input is still streaming.
7. Tool presentation policy is split across five hard-coded tables in two crates.
8. Composer delivery states exist in core but reach the UI as bare strings.

---

## 1. Current pipeline

```
claude CLI (stream-json, --include-partial-messages,           codex app-server (JSON-RPC)
  --forward-subagent-text, --permission-prompt-tool stdio,       providers/codex.rs:903 read_stdout
  initialize{perTaskStopAffordance:true})                        providers/codex/activity.rs:135 Router::observe
  providers/claude.rs:246-266, 391                               providers/codex/wire.rs (item/*, turn/*, error willRetry)
        │ JSON line                                                      │
        ▼                                                                │
providers/claude.rs:729 read_stdout (reader thread)                     │
  ├─ side channels: queue.rs (command_lifecycle), shell_output,         │
  │  mcp server, menu, control replies                                  │
  └─ claude/activity.rs:146 Decoder::decode(line)                       │
        ├─ scope by parent_tool_use_id  :456                            │
        ├─ message() complete blocks    :645  (uuid dedup :628/:1160)   │
        ├─ main_stream() deltas         :942  (message.id+index id :1103-1158)
        ├─ task()/progress()            :1301/:371                      │
        ├─ decision()                   :1403                           │
        └─ wire::parse_events_value     wire.rs:287 (system/*, tool_progress,
                                         tool_use_summary, receipts → ProgressEvent)
        │ Vec<SessionEvent>  (session_event.rs:8; Activity(ActivityEvent) activity.rs:86)
        ▼  bounded sync_channel (backpressure, claude.rs:332, :924)
engine.rs:3145 Engine::pump  (≤256 events/thread/frame, :3213)
  ├─ SessionEvent::Queue → engine/queue.rs:133 Queue::observe → Input::Prompt on Started
  ├─ Activity(..)  → ActivityInput::Observe{generation}  engine.rs:3311  (rejected if stale, activity.rs:1041)
  ├─ everything else → ActivityInput::Main{Input::Event}  engine.rs:3317
  └─ writer.record_event → Store log (store.rs:4002; deltas coalesced store.rs:1426)
        ▼
activity.rs:1032 Activity::apply   ── canonical per-Subject state ──
  SubjectState { records: VecDeque<Record>, seen ids, retracted ids, timings, status, coverage }
  ├─ execution() :1698  dedup by delivery_id :2425, Retract, Snapshot supersede :819
  ├─ append() :572  merge deltas into one record, trim/rebuild
  └─ transcript.apply(Input)  ──►  transcript.rs:1324 Transcript::apply / fold :1415
          Vec<Block{id,body:Body}>  (Body: Paragraph/Heading/Bullet/Code/Tool(ToolBlock)/Visual/
          Prompt/Thinking/Notice/Meta/TurnEnd)  + Status + Progress (progress.rs:128)
        │
        ├─ in-process (demo/tests): cockpit reads Engine directly
        └─ Server → window: protocol.rs:1806 outgoing(ActivityInput) → WireInput{Record|Replay|Live}
              link.rs:445 merge queued deltas → socket → cockpit/mirror.rs:216 protocol::incoming
              → mirror Activity::apply (same code, same Transcript)
        ▼
ferrite-cockpit/src/cockpit.rs:1840 sync_transcript
  reads SubjectView.transcript().blocks(), pending Decision, progress caption
  ├─ transcript_glue.rs:25 transcript_tail → Decision card | failure strip (:35)
  └─ pane::working_tail (pane/working.rs:41) → TranscriptTail
        ▼ TranscriptView::sync (ferrite-pane/src/transcript.rs:436)
ferrite-pane/src/transcript/rows.rs:372 project(blocks) → Vec<Rc<TranscriptRow>>
  RowId {Markdown(run) | Block | Group(first call) | Working | Tail}; reconcile/splices :230/:505
  grouping: groups() :315 (OWN_ROW list), draws_nothing :323, group_at :351
        ▼
pane/text.rs labels: WorkKind::of :47, call_line :244, group_summary :345 (failure counts :442)
pane/output.rs, pane.rs render; GPUI ListState FollowMode::Tail (transcript/scroll.rs:29)
```

The data types at each hop:

| Hop | Type | Identity carried |
|---|---|---|
| CLI → Decoder | `serde_json::Value` line | `uuid`, `message.id`, `parent_tool_use_id`, `tool_use_id`, `task_id`, `agent_id`, `request_id` |
| Decoder → Engine | `SessionEvent` / `ActivityEvent::{MainContent,Content}{id: Option<String>, event: ExecutionEvent}` | delivery id `"{uuid}:{ordinal}"` (claude/activity.rs:1451) or stream id `"stream:{message.id}:{index}"` (:1455); `AgentKey` per child |
| Engine → Activity | `ActivityInput::{Main,Observe{generation},Replay,ReplayEvent}` | Session generation (engine.rs:684, :1064) |
| Activity → Transcript | `transcript::Input` | `ToolBlock.call` = native tool id; `BlockId` minted locally |
| Transcript → pane | `&[Block]` + `Status` + `Progress` | `BlockId`, `markdown_run`, `ToolBlock.call` |
| pane rows | `TranscriptRow{id: RowId, kind: RowKind}` | `RowId` derived from `BlockId`; disclosures keyed by `DisclosureId` (call) |

---

## 2. Gap table

Status values: **has**, **partial**, **missing**, **wrong**, and **n/a** (out of Ferrite's chosen scope; YAGNI).

### 2a. Requirements, sections 4–20

| § | Requirement | Status | Evidence | Fix (files) |
|---|---|---|---|---|
| 4 | App session id vs runtime session id kept apart | has | `ThreadId` vs `Transcript.session_id` from `Init` (transcript.rs:1982); resume by native id (claude.rs:275-280) | — |
| 4 | Event `uuid` used for replay dedup | has | `seen`/`seen_main_frame` (claude/activity.rs:628, :1160); `delivery_id` (activity.rs:2425, :1730) | — |
| 4 | API `message.id` relates stream to completed message | has | `main_stream_message` + `bind_main_stream_block` (claude/activity.rs:947, :1140) | — |
| 4 | User-message UUID ↔ the turn answering it | partial | Queued prompts carry a client UUID (`command_uuid`, claude/queue.rs:41; engine.rs:3587). Direct `send` carries none (engine.rs:5362). Result attribution (`user_message_uuid(s)`) is not read. | Not required by any fixture. Leave it alone unless lane E needs it. |
| 4 | `tool_use.id` ↔ `tool_use_id` | has | `settle_tool` by call (transcript.rs:2189) | — |
| 4 | `parent_tool_use_id` / `agent_id` attribution | has | `scope` (claude/activity.rs:456); `decision` agent_id (:1408-1428) | — |
| 4 | `task_id` / `run_id` / `parent_task_id` | partial | `task_id` aliases (claude/activity.rs:1342). `run_id` and `parent_task_id` are never read (grep: zero hits). | `run_id` guard: §2b #19 |
| 4 | Three meanings of "task" kept apart | has | Todos → `Progress.plan` (progress.rs:179); runtime tasks → `Progress.tasks` or `Subject::Subagent`; no app sessions | — |
| 5 | `assistant`/`user` decoded by content, not outer type | has | Main user text is skipped (claude/activity.rs:696, :742); results become `ToolCompleted` (:822) | — |
| 5 | `stream_event` reconciled against completed messages | has | `TextSnapshot` supersede (activity.rs:819-886) | — |
| 5 | `result` ≠ process end | has | `TurnEnded` vs `Closed` (session_event.rs:106, :199); a non-human origin becomes `BackgroundTurnEnded` (claude/activity.rs:337-345) | — |
| 5 | `tool_progress` | has | `ProgressEvent::Tool` (wire.rs:311-322). Heartbeat and `subagent_retry` are ignored. | — |
| 5 | `tool_use_summary` attached by `preceding_tool_use_ids` | **wrong** | Flattened to `Phase::Working` detail. The ids are dropped (wire.rs:323), so the summary becomes the working-line caption, not a summary of specific calls. | New `SessionEvent::ToolSummary{ids,summary}`. Attach it in transcript.rs (lanes 0, A, B, C). |
| 5 | `system/status`, `api_retry`, `compact_boundary`, `thinking_tokens` | has | wire.rs:342-366 | `status: null` after compaction emits nothing. Phase stays Compacting until the next phase event **(guess: the CLI sends null on completion)**. Map null → `Phase::Working` (lane A). |
| 5 | `system/task_*` | partial | Agent tasks → child `Subject` (claude/activity.rs:1301). Others → `ProgressEvent::Background` (wire.rs:367-395). | §2b #18 and #19 |
| 5 | `background_tasks_changed` replaces the set | has | `BackgroundSnapshot` replace (progress.rs:262) | — |
| 5 | `permission_denied` retained by call id | missing | Not decoded. The denial reaches the row only through the later `tool_result is_error` (wire.rs:1901 test). | Buffer early results instead (§2b #7). Decoding the event is optional. |
| 5 | `hook_*`, `informational`, `session_state_changed`, `commands_changed` | partial | `informational` → Notice (claude/activity.rs:233); state → `RunState` (wire.rs:1325); commands (wire.rs:1313). Hooks ignored. | Hooks: n/a until hooks are surfaced. |
| 5 | Unknown events are forward-compatible | has | `_ => None` (wire.rs:243); test `junk_lines_are_ignored_never_fatal` (wire.rs:2127) | — |
| 6 | `includePartialMessages`, permission broker, `perTaskStopAffordance` | has | claude.rs:252, :264, :391 | — |
| 6 | Only declare dialogs the UI supports | has (by omission) | No `supportedDialogKinds` is sent. Elicitations get `Unsupported` safely (wire.rs:911-929). | — |
| 6 | `forwardSubagentText` / `agentProgressSummaries` deliberate | partial | `--forward-subagent-text` is on (claude.rs:257). Progress summaries are off. | None (deliberate) |
| 7.2 | Delta coalescing that flushes on structure | partial | No timed coalescer. Coalescing is structural: a delta merges into its record (activity.rs:632-642), store records merge (store.rs:1426), and link backlog merges (link.rs:445). The pump drains ≤256/thread/frame (engine.rs:3213). No visible-vs-background cadence. | None needed for fixtures (§2b #44 passes) |
| 7.4 | Completed message supersedes the live one | has | activity.rs:819-886; claude/activity.rs:757-773 | — |
| 7.5 | Tool input ≠ execution; provisional input distinct | partial | State is correct: `content_block_stop` → `ContentBoundary` only (wire.rs:325), and the row is Running until `ToolCompleted`. But no row exists until the completed `assistant` message (wire.rs:710). Only `show_visual` drafts (claude/drafts.rs). | Optional step 3 (§5) |
| 8 | History vs session metadata separate | has | Ferrite's own log is replayed for Main (store.rs:3903). The CLI is resumed for model context only (claude.rs:279). | — |
| 8 | Stable keys; no content-equality dedup | has | Delivery and stream ids. Content equality is used only for repeated *reports* (activity.rs:614-618). | — |
| 8 | Fallback key for frames without uuid | missing | `id: None` → never deduped (activity.rs:1730). | Leave it alone. Claude frames carry uuid **(guess, based on the fixtures)**. |
| 8 | Earlier-history merge / reseed | partial | Child history: `HistoryContent` + dedup + frozen-prefix merge (engine/history.rs:112; test `evicted_child_history_merges_frozen_prefix_and_live_tail_once_without_changing_runtime`, tests/subagent_cockpit.rs:1074). Main never reseeds from the runtime. | — |
| 8 | Superseded-process ownership | has | Per-session receiver (engine.rs:3217-3221). `Observe` is generation-gated (activity.rs:1041). A new `init` session_id resets the Decoder and cancels requests (claude/activity.rs:164-190). | — |
| 9.1 | Explicit tool policy table | **partial** | Five scattered tables: `display_tool_name` (transcript.rs:228), `tool_summary`/`tool_subject`/`result_summary` (transcript.rs:2390-2537), `WorkKind::of` (pane/text.rs:47), `groups()` OWN_ROW (rows.rs:315), and `visual::is_tool`. `AskUserQuestion`/`ExitPlanMode` and todo tools roll into groups. | Registry (§4.2, lanes B and C) |
| 9.2 | Run construction; standalone forces its own run | partial | rows.rs:351-370 `group_at`. Shown thinking *splits* groups (rows.rs:328), whereas Code uses `thinkingJoinsGroup: true`. | Decide product-wise. Not fixture-required. |
| 9.3 | Summary provenance; failure never hidden | has (Ferrite caption) / wrong (provider caption) | The group caption is Ferrite's own and always appends failures (pane/text.rs:345, :441-453). The provider summary is mis-routed (row 5 above). | Lane C: provider caption only alongside the failure suffix |
| 9.4 | No narration folding in Code mode | has | Narration is muted, never hidden (rows.rs:463-472) | — |
| 10 | pending/streaming/complete/incomplete turn | partial | `Status{Idle,Streaming,Blocked,Closed}` (transcript.rs:668). `TurnOutcome{Completed,Interrupted,Error}`. A mid-turn `Closed` sets no outcome (transcript.rs:2069-2083). | §2b #12 |
| 10 | Waiting for permission/question | has | `Status::Blocked` (transcript.rs:2035-2041, :1049); Decision tail (transcript_glue.rs:25) | — |
| 10 | Retrying / compacting | has | `Phase::Retrying/Compacting` notice + caption (transcript.rs:1555-1570; working.rs:435) | Null-status end (row 5) |
| 10 | Main finished, background remains | has | `Progress.tasks` survives `end_turn` (progress.rs:354-362); `working_descendants` (activity.rs:2148) | — |
| 10 | Interruption | has | `aborted*` → Interrupted (wire.rs:1397); TurnEnd row; tools → `Unavailable` (transcript.rs:2100) | `Unavailable` conflates interrupted and stream-lost (lane D, optional) |
| 10 | Stream ended without result | **missing** | `Closed` → Notice + `Unavailable` tools only | §2b #12 |
| 11 | Todo projection from receipts only, child excluded | has | wire.rs:423-480 (successful receipts only); child routing by owner (claude/activity.rs:387-407); progress.rs:179-225 | — |
| 11 | Optimistic create/update and rollback | n/a (design) | Ferrite shows only acknowledged receipts, so there is nothing to roll back. This is simpler than Desktop and satisfies fixtures 28–32. | Add a test for failed TaskCreate (§2b #31) |
| 11 | One owner of the visible checklist | has | `Progress.plan` is the only list; Codex `turn/plan` → `ProgressEvent::Plan` (codex/wire.rs:373) | — |
| 11 | Todo tool calls do not duplicate the checklist | partial | `TaskCreate`/`TaskUpdate`/`TodoWrite` also draw ordinary rows ("Used TaskCreate 3 times") beside the task list (working.rs:290-307) | Registry: `StateOnly` unless failed (proposal, decide) |
| 12–13 | Custom turn plan, wrap-up, next actions | n/a | No `ccd_turn` equivalent exists; none planned | — |
| 14 | Task lifecycle: started/progress/updated/notification | partial | Agents: claude/activity.rs:1377-1400 (status from `patch.status`, ok). Non-agent tasks: wire.rs:367-395 reads the top-level `description`, maps unknown → `TaskStatus::Unknown`, which overwrites (progress.rs:239). | §2b #18 |
| 14 | `run_id` stale-run rejection | missing | — | §2b #19 |
| 14 | Background membership is a snapshot | has | progress.rs:262; test `claude_background_snapshots_replace_tasks_and_exclude_hidden_work` (tests/contract_state.rs:10) | — |
| 14 | `skip_transcript` / `ambient` | has | `hidden_task` (wire.rs:249); excluded (claude/activity.rs:1315); ambient → `ScheduledWork` (wire.rs:280) | — |
| 14 | Background output joins its originating call | missing | `task_notification` `output_file`/resource links are not read. A backgrounded Bash call settles `Ok` at launch. | §2b #22 (deferred) |
| 14 | Per-task stop is real | has | `StopTask` control (session_event.rs:241); `stop_subagent` (claude.rs:628); test `claude_handshake_declares_per_task_stops_so_escape_spares_background_agents` (tests/contract_native_controls.rs:149) | — |
| 14 | Nested transcript ownership | has | `Subject::Subagent(AgentKey)` with its own Transcript (activity.rs:372); aliases (claude/activity.rs:496) | — |
| 15 | Decision resolves once; stale clicks rejected | has | `DecisionHandle{generation,serial}` (activity.rs:241); engine.rs:2643-2648; `submitting` guard | — |
| 15 | AskUserQuestion returns updated input with answers | has | wire.rs:878-880 → `DecisionKind::Questions`; `question_response` (wire.rs:1086) | — |
| 15 | Unknown control request answered, not hung | partial | Unknown `control_request` subtypes are ignored, and the comment admits "leaves the turn hanging" (wire.rs:857-873) | Reply `error` to unknown subtypes (lane A, small) |
| 16 | Findings card; failure stays visible | partial | No card. `ReportFindings` renders as a generic call, and failures show `Failed` (pane/text.rs:198) | n/a (YAGNI) |
| 16 | Human comments as typed data | n/a here | Not part of the transcript pipeline (reviewr crate, not assessed) | — |
| 17 | Capability-gated app tools | has | `show_visual` registered only when offered (claude.rs:301-306) | — |
| 18 | Shared-renderer folding / latest-two | n/a | Not used | — |
| 19.9 | Separate result / exhaustion / interrupt / exit | partial | All except "exhaustion without result" | §2b #12 |
| 19.16 | Merge `task_updated.patch`, replace `background_tasks_changed` | **wrong** / has | Patch is not merged (progress.rs:226-240); snapshot ok | §2b #18 |
| 19.20 | Summaries keep provenance | partial | Row 5 (`tool_use_summary`) | Lanes A, B, C |
| 20 | Stable rows, live tail separate, prepend-safe scroll | has | Splices (rows.rs:505); `Working`/`Tail` row ids; tests `prepend_preserves_the_existing_suffix_rows`, `append_keeps_prior_row_rcs_and_splices_the_tail` (rows.rs:652, :750); expansion keyed by call (transcript.rs:117) | — |
| 20 | VCS/diff side effects not as chat text | has | `FileChanges`, `TurnDiff` (transcript.rs:1621-1648) | — |

### 2b. Section 22 fixtures

| # | Fixture | Status | Evidence / existing test | Fix (files) |
|---|---|---|---|---|
| 1 | Deltas then complete message, same API id → one copy | has | claude/activity.rs:1140; activity.rs:819. `claude_stream_and_complete_block_reconcile_by_native_identity` (tests/contract_fidelity.rs:184); `claude_complete_main_message_without_deltas_is_visible_once` (:140) | — |
| 2 | Partial JSON fragments → stable provisional row | partial | No crash: `input_json_delta` is ignored except for visuals (claude/drafts.rs; `other_tools_never_draft`). No row until the complete message (wire.rs:710). | Step 3: `ToolState::Drafting` from `content_block_start` (claude/activity.rs, transcript.rs, pane/text.rs) |
| 3 | Arg block stops, tool stays running | has | wire.rs:325 → `ContentBoundary`; Running until `ToolCompleted` (transcript.rs:1838) | Add an explicit test (lane B) |
| 4 | Parallel A/B return B/A | has | `settle_tool` by id (transcript.rs:2198). `native_claude_parallel_tool_blocks_are_all_retained` (tests/provider_progress.rs:484) | Add a reversed-order assertion |
| 5 | `user` frame with tool results / replay → no human bubble | has | claude/activity.rs:696, :742; `child_string_prompt_is_preserved_without_becoming_main_input` (claude/activity.rs:1874) | — |
| 6 | Tool fails, incl. specialized card → failure visible | has | `ToolState::Failed`; group "(1 failed)" (pane/text.rs:442); visual `Failed` (transcript.rs:2283) | Keep this true for `StateOnly` in the registry (lane C) |
| 7 | Denial before matching row → attaches later | partial | `settle_tool` on an unknown call returns `None` and the result is lost (transcript.rs:2198-2201). A later `ToolStarted` makes a Running row that ends `Unavailable`. Ordering is normally assistant → user, so this is rare **(guess)**. | Buffer orphan `ToolCompleted` by call id and apply it on `ToolStarted` (transcript.rs, lane B) |
| 8 | Unknown tool / field → generic | has | `WorkKind::Other` (pane/text.rs:63); `junk_lines_are_ignored_never_fatal` (wire.rs:2127) | — |
| 9 | Old runtime emits after replacement | has | activity.rs:1041; claude/activity.rs:164-190; engine.rs:2643 | — |
| 10 | History overlaps live + optimistic echo | has (children) / n/a (Main) | `seen` dedup; `evicted_child_history_merges_…` (tests/subagent_cockpit.rs:1074); `complete_frames_deduplicate_by_delivery_not_shared_message_id` (tests/activity.rs:133). Main prompt echoes are ignored. | — |
| 11 | Earlier history while scrolled up | has | `prepend_preserves_the_existing_suffix_rows` (rows.rs:750); FollowMode suspends (scroll.rs:29) | — |
| 12 | Stream ends without result → incomplete | **missing** | `Closed` mid-turn → Notice only, `turn_outcome` None (transcript.rs:2069-2083). Test `a_closed_session_says_why_and_stops` (transcript.rs:3375) asserts exactly this. | On `Closed` while Streaming/Blocked, push `TurnEnd{Error("ended before the turn finished: …")}` the way `Revived` does (transcript.rs:1781-1793). This makes the failure strip appear (transcript_glue.rs:43). Lanes B and D. |
| 13 | User interrupts → interrupted, pending canceled | has | wire.rs:1397; `remove_subject_decisions` on TurnEnded (activity.rs:1773-1778); `an_interrupted_turn_is_not_an_error` (wire.rs:2174) | — |
| 14 | Background agent outlives foreground result | has | `a_main_that_ends_while_children_work_waits_for_its_resume` (tests/notifications.rs:383); `autonomous_background_completion_does_not_retire_mains_live_work` (tests/subagent_cockpit.rs:373) | — |
| 15 | Open-input interrupt + explicit task stop | has | tests/contract_native_controls.rs:149 | — |
| 16 | Closed-input one-shot interrupt | n/a | Sessions are always open-input; `oneshot.rs` is titles only | — |
| 17 | `background_tasks_changed` empty → cleared | has | progress.rs:262; tests/contract_state.rs:10 | Assert the empty case explicitly |
| 18 | `task_updated` description-only patch preserves fields | **wrong** | wire.rs:372-386: no `status`/`patch.status` → `TaskStatus::Unknown`, so a running task drops out of `working_background`. `label` reads top-level `description`, not `patch.description` (wire.rs:371). Agent path ok (claude/activity.rs:1380). | wire.rs: read `patch.*`. progress.rs:239: an existing task keeps its status when the event says `Unknown`. Map `killed` → Stopped. Lane A. |
| 19 | Late event from prior `run_id` | missing | — | Optional `run` on `ProgressEvent::Background` + `PersistedProgress::Background` (`#[serde(default)]`, store.rs:821). Drop older-run events in progress.rs (lanes 0 and A). Agent tasks: track run per `AgentKey` in claude/activity.rs. |
| 20 | Task before parent | partial | Agent task keyed `task:{id}`, later aliased (claude/activity.rs:1332-1345, :496). `parent_task_id` is ignored. | None until nested background tasks are shown |
| 21 | Ambient / suppressed task → no spinner | has | `claude_ambient_and_hidden_tasks_do_not_create_working_children` (tests/contract_state.rs:64) | — |
| 22 | Launch ack then later outputs | missing | No `output_file`/resource-link join | Defer. No UI consumer yet (YAGNI). |
| 23 | Child text/thinking/todos → nested only | has | `every_child_frame_is_scoped_before_tools_or_usage_can_reach_main` (claude/activity.rs:1604); `child_notifications_cannot_replace_main_heading_or_finish_main_turn` (tests/provider_progress.rs:510) | — |
| 24 | Complete child text without deltas | has | `TranscriptCoverage::Live` (claude/activity.rs:710, :777) | — |
| 25 | Tool summary, task summary, recap near each other | partial | Task summary → child phase (claude/activity.rs:1361). Tool summary → Main caption without ids (wire.rs:323). No recap. Nothing overwrites prose. | `ToolSummary` event, lanes 0/A/B/C |
| 26 | Question submitted/cancelled/interrupted routes once | has | engine.rs:2624-2679; `claude_question_can_be_declined_without_leaving_it_pending` (tests/contract_forms.rs:191); `cancellation_retires_only_its_request_and_clears_waiting_after_the_last_one` (tests/subagent_cockpit.rs:732) | — |
| 27 | Unsupported dialog → cancellation | partial | No kinds are declared (good). An unknown `control_request` gets no reply (wire.rs:866-873). | Reply with an error response for unknown subtypes (claude.rs reader, lane A) |
| 28 | Two TaskCreate, same subject | has | Keyed by the returned task id (wire.rs:425; progress.rs:186); `native_claude_receipts_match_task_identity_and_delete_without_legacy_fallback` (tests/provider_progress.rs:370) | — |
| 29 | Failed TaskUpdate | has | `successful_receipt` (wire.rs:484). No optimistic state. | — |
| 30 | Failed TodoWrite in replay | has | Same; `native_todo_receipt_replaces_the_previous_todo_plan` (tests/contract_plans.rs:59) | Add a failed-receipt case |
| 31 | Failed TaskCreate → chosen behavior tested | partial | Behavior: no row (receipt-only). No test. | Add a test in tests/contract_plans.rs |
| 32 | Deletion + full replacement | has | progress.rs:187-190, :210-225 | — |
| 33–37 | Custom plan / wrap-up | n/a | — | — |
| 35 | Compaction/interrupt mid-plan with child running | has | `end_turn` keeps the plan (progress.rs:354) | — |
| 38–39 | Git/PR action revalidation; reply labels | n/a | Not transcript-owned | — |
| 40 | Findings fail / re-report | partial | Generic row shows Failed | n/a |
| 41–43 | Comments, VM files, artifact schema | n/a | — | — |
| 44 | Hidden session rapid deltas → order preserved | has | Records merge; pump ≤256/frame; link merge (link.rs:445); `a_long_paragraph_streams_in_time_linear_in_its_length` (transcript.rs tests) | — |
| 45 | No-fold mode preserved | has | Narration never hidden (rows.rs:463) | — |
| 46 | Long history streams while a card is focused | has | Row Rc reuse + splices; expansion by `DisclosureId` | — |
| 47 | Retry, compaction, thinking estimates, usage | has | wire.rs:342-366; `UsageDetails{scope:Turn}` (transcript.rs:1739); `captured_claude_api_retry_is_visible` (tests/provider_progress.rs:261) | Null-status end (row 5 of §2a) |
| 48 | Capability changes after init | partial | `commands_changed` handled. The tool list in `init` is not recorded. | n/a until a tool depends on it |
| 49–50 | Fork shares filesystem; truncating resume | n/a | No fork or `resumeSessionAt` | — |

---

## 3. UI-facing gaps the in-flight visual lanes depend on

### Lane C: tool groups

| Need | Core carries it? | Evidence | Gap |
|---|---|---|---|
| Claude Bash `description` | **yes**, as `ToolBlock.title` | transcript.rs:1955-1964 (`title`/`description`/`reason`); used by pane/text.rs:248 | none |
| Tool display titles | **partial** | Only those three input keys. No MCP tool annotation title. Name mapping: `display_tool_name` (transcript.rs:228) plus `WorkKind` (text.rs:47), duplicated across crates. | Registry (§4.2) gives one `display`/`kind` per name |
| Failure state per call | **yes** | `ToolState::{Running,Ok,Unavailable,Failed(msg)}` (transcript.rs:514); test-run failure via `call_failed` (text.rs:474) | `Unavailable` covers both "interrupted" and "session died". Fine for C. |
| `tool_use_summary` | **no** | wire.rs:323 drops `preceding_tool_use_ids` and turns the summary into a working-line caption | Needs core lanes 0, A and B before C can show a provider caption |

### Lane D: live state

| State | Distinguished? | Evidence |
|---|---|---|
| Waiting on permission / question | **yes** | `Status::Blocked` (transcript.rs:2039); `PendingDecision` (activity.rs:248); child `AgentStatus::Waiting` (claude/activity.rs:1434); Decision tail replaces the working row (cockpit.rs:1866-1888) |
| Retrying | **yes** | `Phase::Retrying` with detail "error · attempt n/m · next attempt in Xs" (wire.rs:342-353); Codex `willRetry` (codex/wire.rs:395) |
| Compacting | **yes, with an end-signal gap** | `Phase::Compacting` (wire.rs:355-364); `compact_boundary` → Working "Context compacted" (:365). A `status:null` end is ignored. |
| Interrupted | **yes** | `TurnOutcome::Interrupted` → TurnEnd row (transcript.rs:2004) |
| Stream ended with no result | **no** | Closed mid-turn → Notice only (transcript.rs:2069-2083). No outcome, so `failure_tail` (transcript_glue.rs:43-47) never shows. |
| Background still running after Main | **yes, but wrong on patches** | `Progress::working_background` (progress.rs:337); `ActivityView::working_descendants` (activity.rs:2148); chips in ferrite-cockpit/src/background_chips.rs. Fixture 18 flips running tasks to `Unknown`. |

### Lane E: composer

| Delivery state | In core? | Reaches the UI? |
|---|---|---|
| Held by Ferrite (no session yet) | yes: `Thread.awaiting.prompts` (engine.rs:2475-2478, :3570-3573) | **no**. The composer receives `queued: Vec<&str>` (pane/composer.rs:288; `queue_card` :609). Text only. |
| Submitted, awaiting provider ack | yes: `Queue.admitting` (engine/queue.rs:16) | no |
| Queued (provider-acknowledged) | yes: `QueueEvent::Accepted` → `Queue.items` (engine/queue.rs:146-156) | text only |
| Steered into the running turn (Codex) | yes, encoded as id prefix `steer:` (codex/queue.rs:13-19, :108) | no |
| Picked up (turn started) | yes: `QueueEvent::Started` → `Input::Prompt` (engine.rs:3226-3251) | as the prompt row |

Fix: expose `enum Delivery { Holding, Submitting, Queued, Steering }` beside each queued text from a core accessor (engine/queue.rs plus one cockpit.rs view method). The composer then takes `&[(text, Delivery)]`. A parsed type replaces the string-prefix test. No provider changes are needed.

### Lane B: transcript rows

| Need | Status | Evidence |
|---|---|---|
| Streaming coalescing | **has (structural)** | One growing record per stream id (activity.rs:621-642); one growing Block per section (transcript.rs:2088); link/store merge. No timed cadence, and none is needed. |
| Completed supersedes live | **has** | activity.rs:819-886 (snapshot replaces every record of that stream id and rebuilds); claude/activity.rs:1140-1158 binds by `message.id` + kind, single candidate only. The ambiguous case (two text blocks in one message) falls back to a delivery id, which risks duplication **(guess: untested for multi-text messages)**. |
| Dedup on history load | **has** | `seen` delivery ids, bounded at 8192 (activity.rs:434, `ActivityLimits::dedup_ids_per_subject` :321); Main replays Ferrite's own log, not the CLI's |

Lane B has no core blocker. Its main risk is churn: `rows.rs` is the one place grouping happens, so registry adoption (lane C) and row work (lane B) collide in `rows.rs`. Sequence them (§5).

---

## 4. Proposed architecture for Ferrite

Do not introduce `SessionRuntime`, `TranscriptPresentation` or `AppInteractions` as new modules. Ferrite's existing split already matches handoff §21. What it needs is one missing table and a few corrected reducers.

### 4.1 Canonical state vs projections

| Handoff §21 role | Ferrite owner (keep) | Notes |
|---|---|---|
| `SessionRuntime` | `providers/{claude,codex}` (process, wire, decoder) + `engine.rs` (generations, queue, Decisions, store writer) | Already owns process ownership and generation. Leave it. |
| Durable event log | `store.rs` `LogRecord` | The replay source for Main. Append-only schema with `serde(default)`. |
| **Canonical execution state** | `activity.rs` `Activity` → per-`Subject` `records` + `PendingDecision`s + `AgentInfo` | Dedup, retraction, supersession, rebuild and generation gating all live here. This is the canonical model. |
| Projection: per-subject blocks | `transcript.rs` `Transcript` (a cache rebuilt from records, activity.rs:776) | Also holds `Progress` and runtime counters (`Runtime`, transcript.rs:959). That mixing is acceptable because `Runtime` is already split out for rebuilds. |
| Projection: checklist / background | `progress.rs` `Progress` | Fix the patch semantics. Do not move it. |
| Projection: rows / groups | `ferrite-pane/src/transcript/rows.rs` | Pure function of `&[Block]` + `RowShape` |
| Projection: labels / captions | `ferrite-pane/src/pane/text.rs` | Wording stays in the pane |
| Projection: live line / tails | `pane/working.rs`, `cockpit/transcript_glue.rs` | Derive state, never timers. That already holds. |
| Wire to windows | `protocol.rs` + `cockpit/mirror.rs` (the same `Activity` code runs on both sides) | Every new `SessionEvent` must land in `store.rs` `LogRecord::of` and `protocol.rs:1917` (the exhaustive match enforces this) |

The invariants from handoff §21, with where each already holds:

- One tool identity: `ToolBlock.call` (transcript.rs:1933-1939 ignores a repeated start).
- One visual copy of a streamed message: activity.rs:819.
- A task outlives a turn: progress.rs:360.
- Child ownership is separate: `Subject`.
- Waiting is explicit: `PendingDecision`.
- A summary never hides a failure: text.rs:441. This **must stay true** once a provider caption arrives.

Two invariants are missing: a run outlives a turn with run-ordering (#19), and a stream-lost turn is never left outcome-less (#12).

### 4.2 Tool presentation policy: one table

The policy is currently scattered across transcript.rs:228, :2390-2537, pane/text.rs:47-127, rows.rs:315-318 and `visual::is_tool`. The proposal replaces those with **one** core module, `ferrite-core/src/transcript/tools.rs`: a `match` on name, not a plugin framework.

```rust
pub enum ToolKind { Command, Read, Edit, Write, Search, List, Fetch, WebSearch, Agent, Todo, Question, Visual, Mcp(String), Other }
pub enum Grouping { RollsUp, OwnRow, StandsAlone, StateOnly }
pub struct ToolPolicy { pub kind: ToolKind, pub grouping: Grouping, pub display: &'static str /* or Cow */ }
pub fn policy(name: &str) -> ToolPolicy;
// moved here unchanged: display_tool_name, tool_summary/tool_subject, result_summary, is_command_run/is_test_run callers
```

| Names | kind | grouping | Why (fixture) |
|---|---|---|---|
| `Bash`, `commandExecution`, `BashOutput` | Command | RollsUp | current behavior |
| `Read`, `NotebookRead`, `read_file`, `imageView` | Read | RollsUp | current |
| `Edit`, `MultiEdit`, `NotebookEdit`, `fileChange` | Edit | RollsUp | current |
| `Write` | Write | RollsUp | current |
| `Grep`, `Glob` / `LS` / `WebFetch` / `WebSearch`, `webSearch` | Search / List / Fetch / WebSearch | RollsUp | current |
| `Task`, `Agent`, `collabAgentToolCall` | Agent | OwnRow | current (rows.rs:316) |
| `show_visual` (Claude and Codex names) | Visual | StandsAlone | already `Body::Visual` |
| `AskUserQuestion`, `ExitPlanMode` | Question | StandsAlone | handoff §9.1. Today they join groups. |
| `TodoWrite`, `TaskCreate`, `TaskUpdate`, `TaskGet`, `TaskList`, `TaskStop` | Todo | StateOnly, **but drawn when `Failed`** | fixtures 6 and 32. The checklist owns them. **Product call: confirm before shipping.** |
| `mcp__<server>__*` | Mcp(server) | RollsUp | current |
| anything else | Other | RollsUp | fixture 8 |

rows.rs consumes `policy(name).grouping`: `StateOnly` and not failed means `draws_nothing`, and `StandsAlone`/`OwnRow` breaks a group. text.rs consumes `kind` and keeps its own phrases and verbs. The CLAUDE.md "refactor first" rule applies: extract before adding the todo/question cases, because transcript.rs (4857 lines) and text.rs both encode tool names today.

### 4.3 Module ownership: reuse vs rewrite

| Module | Verdict | Change |
|---|---|---|
| providers/claude/wire.rs | reuse | `task_updated` reads `patch.*`; `killed` → Stopped; `status:null` → Working; `tool_use_summary` → `ToolSummary{ids}`; an error reply for unknown control-request subtypes; optional `run_id` |
| providers/claude/activity.rs | reuse | Route `ToolSummary` by the owner of its first id (like `progress()` :387); per-agent run guard |
| session_event.rs | extend | `ToolSummary { ids: Vec<String>, summary: String }` |
| progress.rs | reuse | Background: preserve status on `Unknown` for known ids; optional `run` ordering |
| store.rs / protocol.rs | extend | Record arms for `ToolSummary`; `run` field `#[serde(default)]`; a gen1 fixture (append-only) |
| activity.rs | **no change** | — |
| transcript.rs | reuse, extract | Extract `tools.rs`. Closed mid-turn → TurnEnd Error. Buffer orphan results. `ToolBlock.caption: Option<String>` set from `ToolSummary` on the last listed call. |
| ferrite-pane rows.rs | reuse | Registry grouping |
| ferrite-pane pane/text.rs | reuse | `WorkKind::of` → `ToolKind`. Group summary: when the newest member has `caption`, use it followed by the existing failure suffix. |
| pane/working.rs, cockpit/transcript_glue.rs | reuse | Failure strip for a session-ended turn (no Retry when `Closed`, already handled at :48, :70) |
| engine/queue.rs, cockpit.rs, pane/composer.rs | extend | `Delivery` enum through to the queue card |

Nothing here needs a rewrite.

---

## 5. Implementation sequence

The lanes are file-disjoint within each step. Each checkpoint runs `cargo test -p <crate>` for the touched crates only, plus the named tests. Step 0 is serial because `SessionEvent` is matched exhaustively in transcript.rs, store.rs and protocol.rs.

**Step 0: contract (one agent, serial).** Files: `session_event.rs`, `store.rs` (record arm and `PersistedProgress::Background.run` with `#[serde(default)]`), `protocol.rs` (match arm and gen1 fixture), `progress.rs` (`ProgressEvent::Background.run: Option<String>`, no logic yet), `transcript.rs` (no-op fold arm).
Checkpoint: `ferrite-core` builds; `protocol/tests.rs` and `tests/store.rs` are green. No fixture passes yet.

**Step 1: three parallel core lanes.**

| Lane | Files (exclusive) | Work | Fixtures made to pass | Tests to add or extend |
|---|---|---|---|---|
| A: provider | `providers/claude/wire.rs`, `providers/claude/activity.rs`, `providers/claude.rs` (reader: unknown control reply), `progress.rs` | Patch merge, killed/paused, null status, `ToolSummary` emission, run guard, unknown-subtype error reply | **18, 19, 27**; keeps 17, 21; feeds 25 | tests/contract_state.rs (description-only patch; empty snapshot; stale run); tests/provider_progress.rs (`tool_use_summary` ids); wire.rs unit test for the unknown subtype |
| B: transcript core | `transcript.rs`, new `transcript/tools.rs`, `docview.rs` callers | Extract the registry (pure move first, its own commit); Closed mid-turn → TurnEnd Error; orphan-result buffer; `ToolBlock.caption` from `ToolSummary` | **12, 7, 25 (core half)**; adds explicit tests for 3 and 4 | Update `a_closed_session_says_why_and_stops` (transcript.rs:3375); new unit tests beside it; tests/contract_tools.rs for policy table coverage |
| E-core: delivery | `engine/queue.rs`, `cockpit.rs` (one accessor), `providers/codex/queue.rs` (expose the steer flag only) | `Delivery` enum per queued item | §10 delivery state (no §22 number) | ferrite-core cockpit tests; tests/contract_state.rs if the steer flag moves |

Checkpoint 1: `cargo test -p ferrite-core`. Specifically: contract_state, provider_progress, contract_fidelity (1 and 5 stay green), subagent_cockpit (9, 10, 14, 26 stay green), contract_plans (28–32, plus a new failed-TaskCreate case for **31**).

**Step 2: UI lanes (after checkpoint 1).** These are the in-flight visual lanes. Each owns its files.

| Lane | Files (exclusive) | Work | Fixtures | Existing tests covering them |
|---|---|---|---|---|
| C: tool groups | `ferrite-pane/src/pane/text.rs`, `ferrite-pane/src/transcript/rows.rs` | Use `tools::policy` for kind and grouping; `StandsAlone` for questions; `StateOnly` todos drawn only when failed; provider caption plus failure suffix | **6, 8, 25 (UI half)**, keeps 45 | rows.rs unit tests (`adjacent_routine_calls_fold_into_one_group` :789, `edits_fold_and_subagents_keep_their_own_rows` :944); cockpit/tests/transcript_rows.rs; cockpit/tests/core_transcript_parity.rs |
| D: live state | `ferrite-pane/src/pane/working.rs`, `ferrite-cockpit/src/cockpit/transcript_glue.rs`, `ferrite-cockpit/src/background_chips.rs` | The session-ended failure strip reads the new outcome; background chips trust corrected statuses; optionally word `Unavailable` as "interrupted" vs "no result" by turn outcome | **12 (visible), 14, 21** | working.rs tests (:553); cockpit/tests/completion_checks.rs; cockpit/tests/subagents.rs |
| E: composer | `ferrite-pane/src/pane/composer.rs`, cockpit composer glue (the `queued:` producer in cockpit.rs) | Show Holding / Sending / Queued / Steering per item | (§10) | cockpit/tests/queued_prompts.rs; cockpit/tests/composer_controls.rs |
| B: rows | `ferrite-pane/src/transcript.rs`, `transcript/scroll.rs` | Only after lane C merges `rows.rs`: no core work is needed for 1, 10, 11, 44 or 46, which already pass | keeps **1, 10, 11, 44, 46** | rows.rs `prepend_preserves_the_existing_suffix_rows` (:750), `append_keeps_prior_row_rcs_and_splices_the_tail` (:652); cockpit/tests/transcript_pointer.rs, render_performance.rs |

Lanes C and B both touch `rows.rs`. Give `rows.rs` to C, and have B work only in `transcript.rs`/`scroll.rs` until C lands.

Checkpoint 2: `cargo test -p ferrite-pane -p ferrite-cockpit`. Then do a visual check of a captured fixture replay (`claude-tool-2.1.243`, `claude-todo-2.1.243`, `claude-error-2.1.243`) in `--demo`/`--load`.

**Step 3 (optional, own checkpoint): provisional tool rows (fixture 2).** This touches core and pane together:
- `ToolState::Drafting` from `content_block_start{tool_use}` (claude/activity.rs `main_stream`, :953).
- A repeated `ToolStarted` upgrades a Drafting row's input (transcript.rs:1933).
- Pane verb wording in text.rs.

Do this only if lane D's live line still feels blind while a long Edit/Write input streams. Otherwise YAGNI.

**Explicitly not doing (n/a in Ferrite's scope):** 16, 22, 33–43, 48–50, a custom turn-plan or wrap-up tool, narration folding, a delta cadence timer, user-message UUID attribution, `parent_task_id` trees.
