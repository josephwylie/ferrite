# Claude Desktop agent transcript: implementation handoff for Ferrite

Prepared 2026-11-03. Primary target: the Code surface in Claude Desktop. Cowork and shared-chat behavior is identified separately throughout.

This document consolidates the substantive findings from the entire conversation: runtime integration, event handling, transcript construction, plans, summaries, agents, questions, comments, outputs, rendering behavior, corrections, compatibility limits, and implementation checks. It is intended to be passed directly to the agent building Ferrite's transcript.

## 1. Scope, evidence, and how to use this document

The desired result is an interactive transcript whose state remains correct during streaming, parallel tool execution, permission waits, interruption, background work, reconnects, and history loading. Fidelity means that equivalent event sequences produce equivalent visible state and available actions. It does not mean identical model wording.

Ferrite's current implementation was not assessed in the work consolidated here. Statements about potential Ferrite issues are checks to perform, not confirmed defects. The receiving implementation agent should apply this material to the actual integration it is authorized to change.

### Version baseline

| Item | Established value | Interpretation |
| --- | --- | --- |
| Installed Claude Desktop | `4.4212.0`, build `4.4212.0` | Source of the Desktop-specific findings |
| Agent SDK version marker in the app | `0.3.309` | `CLAUDE_AGENT_SDK_VERSION` marker in the bundled SDK |
| Published SDK declarations available locally | `0.3.309` and `0.3.317` | Useful for distinguishing public types from app-specific fields |
| Separately installed Claude CLI, reported earlier | `2.1.317` | A separate installation; not proof of the executable used by a Desktop session |
| Actual executable selected by each session | Must be resolved per session | Local, remote, pinned, or externally supplied runtimes may differ |

Source keys such as [H1] and [F1] resolve to files in a temporary source directory. The final source catalog includes archive member names and useful character offsets. The document remains useful without those files; its behavior descriptions and public documentation links are self-contained.

### Evidence labels

- **Public SDK:** supported by the published SDK declarations or documentation for the indicated version.
- **Desktop implementation:** a concrete behavior in the supplied app's host or renderer code. This establishes implementation, not that every account or session enables it.
- **Conditional:** depends on a feature flag, runtime capability, session type, tool registration, or rendering mode.
- **User-visible evidence:** demonstrated by the supplied screenshot or other directly observed output.
- **Proposal:** a recommended Ferrite design or verification rule, not a claim about Claude's implementation.
- **Unresolved:** a detail not established well enough to implement as a compatibility requirement.

Recommended reading order: sections 2–5 first; sections 6–10 for the transcript foundation; sections 11–18 for interactive features; sections 19–23 for integration checks and acceptance criteria. The remaining sections preserve ancillary findings and source pointers.

## 2. Corrections that must survive the handoff

These supersede broader claims made earlier in the conversation.

1. **A tool definition in the app does not establish availability in a session.** Check the live session's tool/capability information. Some app tools are explicitly gated.
2. **`mcp__cowork__send_user_message` is conditional.** Its registration depends on `hasSendUserMessage`; the configuration fallback is disabled. The supplied screenshot shows Claude reporting that this tool is unavailable in that session. This is consistent with the conditional registration, not evidence that the bundled definition is absent.
3. **The screenshot shows a file card and names `SendUserFile` as the available file-delivery tool.** It does not demonstrate `send_user_message`. `SendMessage` targets agents/sessions; `PushNotification` is a separate device-notification mechanism. Preserve these distinctions.
4. **Code has a separate transcript path from the shared Chat/Cowork renderer.** Behavior found in one path must not automatically become a requirement for the others.
5. **The Code narration-folding configuration encountered here is `foldNarration: "never"`.** Classifier and folding functions are present, but this configuration makes that local folding path return no folded/held narration. The shared renderer has another active-capable folding path. Do not describe automatic prose folding as confirmed enabled in the supplied Code configuration.
6. **The “show the latest two rows” implementation belongs to a shared timeline renderer.** It has not been established as the rule for the current Code transcript. Code's own grouping policy is described separately below.
7. **There is no universal public SDK commentary-versus-final-answer tag established here.** `collapsible`, `disposition`, `narration_fold`, `narration_fold_hold`, and display-group fields are presentation metadata, not baseline public SDK promises.
8. **An `ArtifactInput` type is not a universal artifact entitlement.** Tool availability, backing services, account/session context, and renderer implementation remain separate requirements.
9. **Public options are not automatically Desktop defaults.** `forwardSubagentText` and `agentProgressSummaries` are public options. Their names were found in the bundled SDK's plumbing, but they were not found explicitly enabled in the normal Code launch options examined here.
10. **Internal option sightings are not public contracts.** In particular, `forwardNestedSubagentToolCalls` was present in SDK plumbing but not established in the public declarations/docs used here.
11. **No exact parity claim has been established for unseen server behavior or all rollout variants.** Keep verified local rules, conditional features, and proposed Ferrite behavior distinct.

Sources: [H3], [H5], [H6], [F2], [F9], [F11], [D1], [D2].

## 3. The main architectural split

The Agent SDK exposes the Claude Code agent runtime, its stream, and controls. It does not ship the complete Desktop transcript UI. Desktop adds application-defined tools, host state, storage, event routing, presentation rules, and user interactions.

| Capability | Public SDK/runtime contribution | Desktop contribution |
| --- | --- | --- |
| Agent execution | Agent loop, built-in tools, messages and results | Session ownership and app lifecycle |
| Live text/thinking | Partial stream events and completed messages | Buffering, reconciliation, rendering, visibility policy |
| Tool activity | Calls, results, progress and summaries | Labels, icons, details, grouping, failure treatment |
| Todos | `TaskCreate`, `TaskUpdate`, `TaskGet`, `TaskList`, legacy `TodoWrite` when available | Todo projection and pinned plan presentation |
| Rich turn plan | Custom-tool infrastructure | `ccd_turn.update_plan`, scope and linked-session semantics |
| End-of-turn interaction | Custom-tool infrastructure; some runtimes also expose recognized built-in variants | Recap, waiting state, validated actions, sidebar status |
| Subagents | Invocation, task events, attribution, optional text forwarding | Nested transcripts, activity rows, agent navigation |
| Background work | Task lifecycle and authoritative live-task set | Activity indicators, per-task controls, session summaries |
| Questions/permissions | `AskUserQuestion`, permission callbacks and control responses | Forms, approval cards, waiting state and answer delivery |
| Review findings | Structured `ReportFindings` data where available | Findings cards, file grouping, annotations and navigation |
| Human comments | Receives comments as conversation input | Anchors, threads, editing, serialization and submission |
| Outputs | Tool results, resources and resource links | File cards, previews, downloads and output registry |
| Artifacts/drafts/widgets | General tool extension mechanism; published artifact schema in the checked SDK | Editors, rendered content, screenshots and widget context |
| History | Runtime persistence, resume/fork and transcript access | Sidebar metadata, history paging and UI-state restoration |
| File rewind | Checkpointing/rewind support | Selection UI and coordination with conversation history |
| Git/PR state | Relevant tool results and, in some runtimes, VCS events | Diff invalidation, PR tracking and available-action calculation |
| Usage/retries/compaction | Structured runtime events and usage data | Status labels, badges, notices and progress presentation |

App-specific tools are implemented using the same general custom-tool mechanism available to applications: tool definitions plus handlers registered through an SDK MCP server. A Ferrite implementation can supply its own equivalents. [Custom tools documentation](https://code.claude.com/docs/en/agent-sdk/custom-tools)

## 4. Keep identities and entities separate

The implementation needs more than a single list of chat strings.

### Identities

| Identity | Purpose |
| --- | --- |
| Desktop/Ferrite session ID | App-owned workspace/session record |
| SDK `session_id` / host `cliSessionId` | Runtime conversation and resume identity |
| Event/message `uuid` | Delivery/history identity; useful for replay deduplication |
| API assistant `message.id` | Relates a streaming message to its completed representation |
| User-message UUID | Relates a sent or steered input to the work answering it |
| `tool_use.id` / result `tool_use_id` | Joins a tool call to its result |
| `parent_tool_use_id` | Attributes nested messages to the spawning tool call |
| `agent_id` | Agent identity on applicable complete messages/events |
| `task_id` | Runtime task identity |
| `run_id` | One execution of a task; changes when that task resumes |
| `parent_task_id` | Runtime task hierarchy when supplied |
| Todo/task-list item ID | An item in the agent's written checklist |
| Plan-row ID | App plan identity, generated separately from runtime task IDs |
| Artifact/file/comment anchor ID | Identity of a user-visible output or review target |

The host keeps an app session ID and a runtime session ID separately. A conversation fork and a filesystem worktree are also distinct. Resuming/forking a conversation does not by itself isolate files.

### Three different meanings of “task”

1. **Todo:** a written checklist item, such as “Run tests.”
2. **Runtime task:** an executing subagent, background shell, workflow, monitor, or background MCP operation.
3. **Desktop session/plan step:** an app-owned unit that may refer to another session.

Use explicit relations between them. An identical label is not a join key. A completed tool invocation can launch work that remains active as a runtime task.

### Input-to-turn attribution

The SDK can echo `user_message_uuid` and, in newer versions, `user_message_uuids` on response/result frames. Multiple inputs can be incorporated into one turn. Some autonomous or resumed work answers no newly sent input. Child-agent frames have different attribution.

Proposal: persist the UUID of each outgoing user message, track acknowledgement/pickup separately from submission, and use explicit response attribution where available. Avoid a permanent assumption that every input yields exactly one isolated response.

Sources: [H1], [F1], [F3], [D1].

## 5. Public event families the transcript must understand

The following is a practical event inventory, not a replacement for the pinned SDK's complete type union.

| Event | Important data and interpretation |
| --- | --- |
| `assistant` | Complete assistant content blocks: text, thinking, tool calls and other supported content; retain attribution and model/message identity |
| `user` | May be a human message, a replay, tool results, or synthetic/meta input; the outer type alone does not mean “draw a user bubble” |
| `stream_event` | Raw incremental event in `event`; reconcile against completed messages |
| `result` | Turn-level outcome, usage and related metadata; not synonymous with “the entire long-lived session process ended” |
| `tool_progress` | `tool_use_id`, `tool_name`, elapsed seconds, optional task ID and parent attribution; can include heartbeat/retry metadata |
| `tool_use_summary` | `summary` plus `preceding_tool_use_ids`; relates a status summary to specific tool calls |
| `system/init` | Runtime/session initialization and tools/capability information |
| `system/status` | Runtime status changes, including states useful for waiting/compaction UI |
| `system/api_retry` | Retry state; do not convert every retry into a terminal failure |
| `system/compact_boundary` | Context compaction boundary; distinct from a new conversation |
| `system/hook_started`, `hook_progress`, `hook_response` | Hook lifecycle where supported/enabled |
| `system/informational`, notifications | Structured notices; preserve severity and relevant correlation |
| `system/task_started` | Task creation/registration and optional background, hierarchy and agent-type data |
| `system/task_progress` | Task usage, latest tool and optional summary |
| `system/task_updated` | A patch to one task's state |
| `system/task_notification` | Completed/failed/stopped task outcome and possible output references |
| `system/background_tasks_changed` | Full authoritative set of live background tasks |
| `system/permission_denied` | A denial that may arrive before the corresponding row/result; retain by tool-call ID |
| `system/thinking_tokens` | Thinking progress estimates; distinct from delivered prose |
| `system/commands_changed` | Live command capability changes |
| `system/session_state_changed` | Session state updates when supported |

The supplied host also recognizes additional runtime-specific families: VCS/code-publication changes, plugin installation, file persistence, model fallback/refusal, local command output, memory events, command lifecycle, UI notices/pane pushes, and app-specific session coordination. Treat unknown events as forward-compatible data rather than crashing or rendering raw objects into the chat.

### Tool progress and summaries

The checked `SDKToolProgressMessage` declaration includes optional `heartbeat`, `subagent_type`, and `subagent_retry` fields. Retry metadata contains agent ID, attempt/max retries, delay, HTTP-status-like information when available, and an error category. A heartbeat is liveness, not proof that a new action completed.

`SDKToolUseSummaryMessage` contains `summary`, `preceding_tool_use_ids`, `uuid`, and `session_id`. This is distinct from assistant prose, thinking summaries, and task-progress summaries. The UI should attach it using the supplied IDs rather than “the last tool on screen.”

Sources: [D1], [H1], [F1].

## 6. How the Code host configures the SDK

The normal local Code launch constructs a substantial options object before starting the query. The following settings were established in that path; conditional or app-specific fields are identified.

| Setting | Supplied behavior | Ferrite implication |
| --- | --- | --- |
| `cwd` | Session/worktree directory | Resolve the actual work location before starting the runtime |
| `model`, `effort` | Session-selected/inherited settings | Keep requested and effective values distinguishable |
| `thinking` | Adaptive form under capability/model conditions | Gate on the actual runtime and selected model |
| `systemPrompt` | Normally `type: "preset", preset: "claude_code"`, with an app append; custom override path also exists | Use the intended starting prompt explicitly |
| `settingSources` | `['user', 'project', 'local']` | Project instructions/settings are part of the effective agent behavior |
| `includePartialMessages` | `true` | Enables the live stream used by the transcript |
| `enableFileCheckpointing` | `true` | Supports file-rewind features; not itself a diff viewer |
| `canUseTool` | App permission broker callback | Bridge actual user decisions to the pending runtime request |
| `onUserDialog` | App dialog broker callback | A separate dialog mechanism from ordinary tool approvals |
| `supportedDialogKinds` | Calculated from what the app can display | Declaring a callback alone is not the full capability handshake |
| `perTaskStopAffordance` | `true` | The host implements individual task controls; interruption semantics depend on this |
| `permissionMode` | Session-selected/effective mode | Represent effective permission state in the UI |
| `allowDangerouslySkipPermissions` | Calculated, not universally enabled | Its presence is not a recommendation to enable bypass mode |
| `env` | Explicit constructed environment | Preserve required inherited variables when supplying an SDK environment |
| `hooks` | App hooks, modified for session/backend/configuration | Some app behaviors rely on hooks as well as tools |
| `mcpServers` | App tools plus connector/plugin servers | Tool registration and enabled capabilities must agree |
| `additionalDirectories` | Selected/managed extra roots | Keep actual access and displayed project context aligned |
| `stderr` | Diagnostic capture callback | Process diagnostics must not be mixed into model text |
| `toolConfig.askUserQuestion.previewFormat` | HTML preview under an app setting | Preview capability is conditional |
| Prompt suggestions | Conditionally enabled; can be disabled when the turn box is mounted | Avoid competing next-step surfaces |

Other fields seen include managed settings, dialog-specific prompts, `rapidFollowupPreempt`, authentication-refresh callbacks, launcher-specific state and internal extra arguments. These are not all established as public options that Ferrite should copy. Use published support for the chosen SDK/runtime version.

### Prompt and settings implications

The SDK's minimal default differs from the normal `claude_code` preset. The app adds instructions for its own tools and interaction flow to the preset. A Ferrite prompt should describe Ferrite's real tools and capabilities, and remain consistent with the actual registered schemas. [System-prompt documentation](https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts)

In the checked `0.3.309` declaration, omitted `settingSources` loads all sources, while an explicit empty list selects isolation. The `project` source is required for the documented project-instruction behavior. Defaults have varied across SDK history; check the pinned release instead of relying on an older assumption.

The app's options also depend on whether the runtime is local, remote, a host launcher, a spare/warm query, or a session with different built-in tool support. The launch path can reuse a prepared query and must dispose it if an archive/clear/rewind or cancellation wins a race before installation.

Sources: [H1] around 745,662–768,626 and 2,199,949; [H3]; [D1].

## 7. Stream handling, buffering, and rendering cadence

### 7.1 Host pipeline

The host consumes the SDK query iterator and routes events through session handling. It emits two important app-level event shapes:

```text
{ type: 'message', sessionId, message }
{ type: 'session_updated', sessionId, session, ... }
```

Transcript content and session/sidebar state therefore have separate update channels.

Partial stream events take an early path through a delta coalescer. Complete messages receive timestamps when absent, are added to the message buffer unless already represented by a known replay, update host bookkeeping, and are emitted to the renderer. A host query-identity check prevents many stale-process updates from taking ownership of a replacement session.

### 7.2 Delta coalescing

The host coalescer groups compatible `content_block_delta` chunks:

| Delta kind | Concatenated field |
| --- | --- |
| `text_delta` | `text` |
| `input_json_delta` | `partial_json` |
| `thinking_delta` | `thinking` |

Compatibility requires the same delta field, content-block index, and parent tool-use identity. A different field/index/parent flushes the previous batch. Non-delta events and non-stream events flush the batch before being forwarded. This preserves structural ordering around starts, stops, results and other events.

The interval callback returns 16 ms for an on-screen session, with additional app conditions that also choose the fast path. The background path uses 250 ms. These are supplied implementation values, not SDK protocol requirements.

### 7.3 Renderer stream buffer

Code maintains a per-session live buffer with:

- Current API message ID and last cleared message ID.
- Main-content blocks and thinking blocks.
- Last event/commit times, a commit timer and visibility checks.
- Listener registrations and whether thinking content is wanted.
- A separately cached live snapshot.

`message_start` resets the current stream buffer. Block starts/deltas schedule changes; block stops/message stops flush pending commits. The examined buffer is marked `dont_smooth = true`; telemetry in that path describes a 33 ms interval and pass-through behavior. Avoid interpreting this as a requirement to invent additional artificial token pacing.

The main streaming buffer handles the main-session attribution path. Completed subagent messages are handled by a separate nested transcript path.

### 7.4 Completed messages supersede the live representation

The transcript store tracks a streaming anchor using API message identity. When the completed assistant message with the matching identity arrives, the store clears/supersedes that live anchor. A completed message should not be appended as a second visual copy of the same streamed text.

Preserve the distinction between:

- Event UUID.
- API message ID.
- Tool-call ID.
- User-message UUID being answered.

These identities solve different reconciliation problems.

### 7.5 Tool input is not tool execution

Tool arguments can arrive as partial JSON. `content_block_stop` means the argument block finished generating; it does not mean the tool succeeded. Completion/error comes from the correlated tool result or the applicable background-task lifecycle. [Streaming output documentation](https://code.claude.com/docs/en/agent-sdk/streaming-output)

Proposal: represent provisional input, admitted/running work, user wait, and settled output separately. Partial JSON must not crash a row renderer or be treated as a complete validated tool request.

Sources: [H1] around 1,269,909–1,270,107 and 1,586,995; [H2] function `iP` around 669,474; [F1] around 184,400–186,800 and 239,700–242,000.

## 8. History, deduplication, incremental parsing, and session storage

### Transcript versus session metadata

Runtime conversation history uses Claude Code transcript persistence. The Desktop host also has its own session metadata persistence, with `local-sessions.json` identified as a metadata file name. The renderer's transcript state, live streaming buffer, and sidebar/session state are separate projections.

Public session support can resume a specific conversation by ID and fork conversation history. A conversation fork does not fork the filesystem. [Session documentation](https://code.claude.com/docs/en/agent-sdk/sessions)

### Message reconciliation

The Code store has handling for live arrival, initial seed, reseed and gap/history insertion. It tracks message keys, ordering sequences, optimistic sequences, and epochs. Incoming history may precede already displayed live messages.

Observed mechanics include:

- Stable UUID-based keys where available.
- A deterministic fallback key for messages without a usable UUID, derived from selected content fields.
- Optimistic user-message reconciliation when an authoritative echo arrives.
- Ordered merging of earlier history and live content.
- A `changedFromIdx` that limits downstream recomputation.
- Clearing a streaming anchor when the matching completed API message arrives.
- Explicit handling of terminal/result observations during reconciliation.

Proposal: preserve event identity and source/order information. Content equality alone is not a safe deduplication rule: two legitimate tool calls or messages can have identical text.

### Incremental parser state

The Code parser maintains maps/sets for pending tools, pending tool-entry indexes, task-event items and entry indexes, foreground tasks, delayed permission denials, host results, open assistant entries, setup failures, refusal rows, peer-message holds, disk-sourced subagent activity, task subjects/details, agent descriptions and model switches.

Completed portions can remain stable while unresolved entries are revisited. WeakMap caches and equality checks preserve object identity where a row has not changed. This is an important part of keeping a large transcript responsive.

The nested-agent parser similarly maintains pending calls, owned tool IDs, final response, launch failure, resumed-run state, prompt and entry IDs. It can merge adjacent assistant items, merge adjacent tool groups, and attach preceding thinking to the following tool group.

### Reconnect and history anchors

The host tracks renderer-known UUIDs and anchors, with a bounded recent-window path of 2,000 messages in the code encountered. Depending on the available anchor and missing data, it can skip work, send content from an anchor, or require a whole relevant window. Remote adoption also avoids re-emitting already known rows.

This is evidence for explicit resynchronization support, not a recommendation that Ferrite use exactly 2,000 entries or assume all history fits in memory.

### Session update batching

Session metadata updates are batched separately from transcript deltas. The host compares formatted state, schedules work at microtask boundaries, and flushes pending session updates before unrelated event dispatch where needed. The frontend updates its session list on `session_updated`, lifecycle events, permission events and other relevant notifications. Message events do not all force a complete sidebar refresh.

### Working directories and transcript locations

The host has explicit handling for transcript location changes around worktrees and remote mirrors. Main transcript files and subagent files are distinct. Remote synchronization tracks offsets/sizes, handles missing files and stale generations, and can synchronize child transcripts independently.

Proposal: a UI should resume from a valid runtime identity and transcript source, not reconstruct the agent's context by concatenating visible chat bubbles. Record the relationship among session ID, runtime ID, work directory and worktree.

Sources: [H1], [F1], [F3], [F5], [F6], [F12].

## 9. Code transcript grouping and presentation

### 9.1 Tool policy is explicit

Code classifies tools before constructing activity runs. The policy vocabulary includes:

- `rolls-up`: can participate in ordinary activity runs.
- `own-run`: gets a dedicated run category.
- `stands-alone`: renders independently.
- `as-words`: can appear as user-facing prose.
- `lifted-out`: is promoted outside the ordinary run.
- `drawn: nothing`: contributes state without an ordinary visible row in that mode.

Established categories include:

| Category | Treatment found |
| --- | --- |
| Plain tool, peer send, worktree move, subagent | Ordinary roll-up |
| Suggested task | Dedicated run |
| Chapter marker | Lifted out |
| `AskUserQuestion`, `ExitPlanMode` | Standalone |
| `SendUserMessage`, `SendUserFile` | Standalone send-to-user category; eligible content can appear as words under another mode |
| User-message representation | As words |
| Push notification | Ordinary roll-up when delivered; unsent notification can stand alone |
| Turn wrap-up | Standalone |
| Workflow, artifact surface, `ReportFindings`, design output | Standalone |
| Credential request, connector suggestion, plugin/skill card, skill proposal | Standalone |
| MCP app, approved registry card, verification-code entry | Standalone |
| Viewed/started session and coordinator spawn | Standalone |
| Own-session start or streaming session start | Standalone category with no ordinary row in the indicated mode |
| Offer brief | Rolls up, with no ordinary row in the indicated mode |
| Some specialized app calls | Separate run category, conditionally merged back into ordinary runs |

The exact result depends on tool state and rendering context. For example, an unsettled standalone operation may temporarily roll into activity, and send-to-user content is only eligible for direct prose treatment when its state/content permit it.

### 9.2 Run construction

Adjacent calls of a compatible category join a run. Standalone calls force their own run. Additional boundaries include explicit pins/chapters and task-event placement. Task events and notices retain an insertion position relative to the calls in a run; they are not simply appended at the end of the latest group.

The run model also supports orphan notices/events, promoted output cards, leading/trailing content, file-path hints, and stable row identity. The receiving agent should reproduce the required observable grouping rules with a small explicit policy table, then add specialized cases as supported features require them.

### 9.3 Labels and summaries

Labels can be drawn from the tool name, parameters, description, supplied display title, structured result metadata, or a recognized app-specific operation. File operations, shell commands, browser/computer actions and integration tools have different formatting rules. Browser/computer label helpers distinguish actions such as navigation, typing, clicking, scrolling, screenshots and keyboard input, with state-sensitive wording.

Several distinct summary sources exist:

1. Ordinary assistant prose.
2. Thinking summaries or a generic thinking state.
3. `tool_use_summary`, associated with specified call IDs.
4. Subagent `task_progress.summary`.
5. App/server-supplied run/group summaries.
6. Locally constructed descriptions of the tools in a group.
7. Turn recap from a wrap-up tool.

Keep their provenance. A successful-sounding caption must not erase a known tool failure. The shared presentation code also accounts for failed counts, all-failed versus partly failed groups, declined permissions and localized summary language.

### 9.4 Code narration configuration

The Code transcript source defines a configuration object with `thinkingJoinsGroup: true` and `foldNarration: "never"`. Its folding function returns empty folded/held sets under that setting.

The same source contains dormant-capable logic using a local text classifier, a score threshold, and guards for links, questions, markup, length, adjacency and already-shown text. This is useful design information, but it is not an enabled behavior requirement for this Code configuration.

Detailed folding rules and shared-renderer differences are preserved in section 18.

Sources: [F2] around 849,400–855,000, 1,388,258–1,405,545; [F4]; [F5].

## 10. Loading, waiting, interruption, and failure

Code's turn presentation can distinguish `pending`, `streaming`, `complete`, and `incomplete`. Whether a turn stays open also depends on live items, active activity groups and its position in the conversation. A user message has its own delivery state, including sending or steered/awaiting-pickup behavior.

The host's session-running state and the transcript's current streaming state are related but not identical. The host can mark a session active when an assistant response starts, maintain pending turn bookkeeping, and report a stream that ended without the expected result as incomplete/error rather than successful.

Important state distinctions:

- Generating text or tool input.
- Executing a foreground tool.
- Waiting for a permission answer, question answer, authentication step, or dialog.
- Retrying a model/API request.
- Compacting context.
- Setting up the workspace or waiting for a worker.
- Main turn finished while background work remains.
- User interruption/cancellation.
- Tool failure, task failure, stream loss, or process failure.
- Session disconnected but reconnectable.

The host records a `stream_ended_no_result` category when appropriate and has query-crash/recovery bookkeeping. A changed/stale query is not allowed to own cleanup for the replacement query.

The nested transcript path can mark pending tools interrupted when the corresponding work really ended, with a visible incomplete/error representation. It also distinguishes background-agent handback, resumed runs, launch failures and final responses.

Proposal: derive loading states from execution state and pending requests. A timer can format elapsed time, but should not determine success. Likewise, absence of text tokens is not enough to conclude that work is hung; tools, retries and thinking may be active.

Sources: [H1], [F1], [F2] function `xG` around 926,902, [F3].

## 11. Native todos and task-list projection

The native checklist is a projection of tool calls and their results. It is separate from executing background tasks and from the app's richer turn plan.

### Recognized tools and schemas

The small Desktop todo reducer recognizes `TodoWrite`, `TaskCreate`, `TaskUpdate`, `TaskGet`, `TaskList`, and `TaskStop`. In that reducer, only `TodoWrite`, `TaskCreate`, and `TaskUpdate` directly change the displayed list. Recognition of the other names does not establish that they mutate todos.

Public input shapes in the checked declarations:

- `TodoWrite`: `todos` containing `content`, `status`, and `activeForm`.
- `TaskCreate`: required `subject` and `description`; optional `activeForm` and metadata.
- `TaskUpdate`: required `taskId`; optional subject, description, active form, owner, metadata, status, and dependency changes. Status can be `pending`, `in_progress`, `completed`, or `deleted`. `addBlocks` and `addBlockedBy` add dependency relationships. Metadata is merged, with null values used to remove keys.

The UI reducer does not expose every field in these schemas. In particular, a simple checklist is not a complete task-dependency editor.

### Replacement and optimistic updates

Only main-session messages participate in the main checklist: messages with a non-null `parent_tool_use_id` are ignored by this reducer.

| Operation | Established projection behavior |
| --- | --- |
| `TodoWrite` | Clears the existing map and replaces it with the supplied list |
| Legacy item text | Accepts `content`, falling back to `text` |
| Missing legacy item ID | Uses `todo-${index}` |
| Unknown item status | Normalizes to `pending` |
| `TaskCreate` | Creates a pending row immediately from `subject` |
| Temporary creation identity | Uses the tool-call ID, with a `task-${map.size}` fallback |
| Successful creation result | Rekeys the temporary row to the returned task ID while preserving its position |
| `TaskUpdate` | Patches supplied fields while preserving unspecified fields |
| Update of an unknown task | Creates a provisional row, using supplied subject or `Task #<id>` with the ID display limited to 64 characters |
| `status: deleted` | Removes the item |

Creation reconciliation uses `tool_use_result.task.id` or the camel-case wrapper equivalent when available. A message-level wrapper is used only when exactly one matching creation result is present; otherwise the reducer falls back to the individual result content. It keeps a call-ID-to-temporary-ID map. Matching by title would be incorrect when two tasks have the same subject.

### Failure and replay handling

- Failed `TodoWrite` calls are identified by correlated error results and skipped when replaying the list.
- Before `TaskUpdate`, the reducer saves the previous item and position by tool-call ID.
- A failed update restores that prior item and position.
- A structured `success: false` result with the relevant `taskId` also counts as failure.
- For a provisionally created unknown task, a structured `Task not found` error removes the provisional item; other failure paths can return it to pending.
- **Rollback of failed `TaskCreate` was not established in this small reducer.** Do not generalize update rollback into a claim that every optimistic creation is correctly removed. Ferrite should make its intended behavior explicit and test it.

### Availability is version/model dependent

The public todo documentation describes a change starting at CLI `2.1.268` / SDK `0.3.268`: legacy todo tools are enabled by default for Opus 4.7, Sonnet 4.6, Haiku 4.5 and older models, while newer models require explicit opt-in. The documented opt-in mechanisms include naming the relevant tools in `allowedTools`, an explicit `tools` selection, or `CLAUDE_CODE_ENABLE_TODO_TOOLS=1`. `CLAUDE_CODE_ENABLE_TASKS=0` selects the legacy `TodoWrite` path. These are release-specific behavior, not a timeless assumption about every future model. [Todo tracking documentation](https://code.claude.com/docs/en/agent-sdk/todo-tracking)

When supplying `env`, preserve necessary inherited environment variables; the TypeScript SDK option is a replacement environment. Separately, `allowedTools` is a permission mechanism, not a general guarantee that all other tools disappear.

Proposal: choose a canonical visible checklist strategy. If Ferrite supports both native todos and a custom turn-plan tool, define which one owns the pinned plan in a given session. Do not accidentally show two independent lists for the same work.

Sources: [F7] contains the complete small reducer; [D1], [D2].

## 12. Code's custom turn-plan tool

### Registration and capability detection

The host defines tools in an SDK MCP server named `ccd_turn`. The names seen by the agent include:

```text
mcp__ccd_turn__update_plan
mcp__ccd_turn__wrap_up
```

Registration is conditional on a Code session, an enabled turn box, and an additional feature gate. A `noWrapUp` option omits the wrap-up tool. These tools are marked `alwaysLoad: true` in the applicable definitions. Their `readOnlyHint` annotation concerns their app-state operation; it does not mean the entire agent session is read-only.

The app also recognizes runtime-provided variants, including a built-in `WrapUp`, and avoids redundant custom registration when the runtime already supplies the relevant support.

The frontend detects support from the main `system/init` tools list, keeping distinctions such as no tools listed, update-plan omitted, and wrap-up omitted. Nested messages do not become authority for the main session's capabilities. Recognized assistant tool calls can supply fallback evidence under the code's capability rules.

### Input contract

The custom plan accepts this shape:

```typescript
type PlanStatus = "pending" | "in_progress" | "completed";
type UpdatePlanInput = {
  scope?: "first_task" | "continuation" | "new_task";
  items?: Array<{
    content: string;
    status: PlanStatus;
    session?: string;
  }>;
  advance?: boolean;
  done?: number[];
  start?: number;
};
```

This illustrates the app tool contract, not a public SDK built-in interface.

| Field/rule | Behavior |
| --- | --- |
| `items` | Replaces the plan; between 1 and 12 items |
| `scope` | Required when supplying `items` |
| Item wording | Validated through a 160-character helper; prompt guidance favors short steps |
| `session` | Optional linked-session slug; lower-case alphanumeric/hyphen format, up to 24 characters |
| `done` | One-based indexes of steps to complete |
| `start` | One-based index of the step to mark in progress |
| `advance` | Completes the first in-progress item, then starts the next pending item after it, or the first pending item if necessary |
| Replacement plus incremental fields | Replacement wins; other updates are ignored with a warning |
| Invalid indexes | Validated as integers against the known plan size, or the supported maximum when size is unavailable |
| More than one in-progress item | Prompt discourages it; handler warns rather than universally rejecting it |

The incremental mutation does not automatically reset every other in-progress item. Preserve this distinction between prompt guidance and validation behavior.

### Host and renderer own different parts

The host keeps plan statuses in a WeakMap keyed by the session object and associated runtime session ID. It does not keep the plan wording in that state. The renderer derives wording and rows from the transcript. Frontend row IDs use `plan-${index}` in the path encountered.

The host tracks prompt identity and whether the current prompt has wrapped up. Main-agent ownership matters: children do not update the main plan. A `PostToolUse` hook records the prompt ID only for the main agent. Session-start hooks account for compact/clear transitions.

After compaction, a reminder is relevant only when it is still the same prompt, work remains unfinished, and that prompt has not wrapped up. Since the host only retained statuses, the model may need to reconstruct wording from its conversation summary.

### Prompt guidance that affects behavior

The app's instructions encourage an initial plan at the first meaningful tool action, batching the plan call with real work when possible, short steps, and updates after user steering or changed scope. Continuation includes acting on the preceding result; genuinely separate work can be marked as a new task. These are behavioral instructions layered on top of the tool schema, not automatic SDK behavior.

Ferrite can implement equivalent guidance for its actual tools. Copying a schema alone will not produce the same planning habits.

### Transcript replay and completion

The plan projector locates the latest successful replacement and replays later incremental updates in order. Known failed plan calls are skipped. Successful applicable wrap-up can settle plan steps while retaining a pre-wrap-up plan snapshot.

Observed derived state includes:

```text
plan, planBeforeWrapUp, planTouchedThisTurn, planTurnEnded,
planCutShort, planFinished, compactedSincePlan, turnSettled,
agentsRunning, workedWithoutWrapUp, proposedTaskIds,
wrapUp, wrapUpCallId, closingMessageFollows, wrapUpReceipt
```

The projector accounts for interruption, compaction, active agents, and work completed without a wrap-up. It is not equivalent to marking all items completed whenever any `result` arrives.

Sources: [H2] plan/turn tools near the beginning of the file; [F3] capability handling around 39,019–41,000; [F4]; [F6] around 23,867–24,600.

## 13. Wrap-up, next actions, and end-of-turn behavior

The turn box is an app interaction model. It combines a concise recap, an optional question or blocking state, and optional actions validated against current app state.

### Shape and action vocabulary

The current contract uses required `recap`, optional `ask`, optional `blocked`, and optional action entries containing an action kind and/or reply label, subject to the handler's validation rules. Recognized app action kinds include:

```text
commit, create_pr, merge, fix_ci, address_comments,
resolve_conflicts, verify, archive
```

A legacy `new_session` action is recognized in compatibility code but is being replaced by a dedicated task/session proposal card. Do not make a retired action central to a new integration.

At most two buttons are intended; the first is the recommended action. For reply-style actions, the visible label is the text sent back. A legacy separate `message` field is parsed for compatibility, but the current tool warns against relying on hidden reply payloads.

### Guidance versus validation limits

- Recap guidance targets roughly 300 characters and two or three sentences, around 40 words.
- Question guidance is approximately 100 characters.
- Labels target at most 24 characters and one to three words, with four as an upper guidance limit.
- Parsing includes a more permissive fallback cap of 1,200 characters for recap-like content.
- Sidebar description uses a normalized, truncated first sentence, with a 200-character limit in the observed path.

Do not confuse a prompt's preferred length with the parser's maximum accepted input.

### Important rendering rules

| Situation | Consequence |
| --- | --- |
| Recap only, no question/actions, not blocked | Can update status without displaying a next-step box |
| `blocked: true` | Represents waiting for the user; must remain distinct from successful completion |
| Action unavailable when submitted | Backend rejects it so the agent can correct the request |
| Action becomes unavailable later | Frontend can remove it; invalid primary action can also remove dependent buttons/question, with blocked-state considerations |
| Visible reply label starts with `/` or `!` | Rejected or removed in applicable validation paths |
| Invisible/control characters or irregular whitespace | Normalized/filtered; Unicode normalization is applied |
| Tool is not supported in the viewing surface | Agent may need ordinary prose for the user to receive the information |

Compatibility parsing recognizes older fields such as `outcome`, `next_ask`, `next_brief`, `next_actions`, and `needs_user`. A new Ferrite tool should have one clear current schema, with migration code only where needed.

### Ending the turn is conditional

The host can return `_meta: { "claude/endTurn": true }` under the relevant gate and visibility/closing-message conditions. This is not a universal MCP or SDK guarantee. In other cases, an ordinary final answer follows the wrap-up tool call.

The renderer retains a `closingMessageFollows` distinction. A wrap-up recap should not automatically erase the final answer or create a duplicate copy of it. Remote-control surfaces may not display the turn box, so a plain-text fallback is significant.

The host saves turn-wrap-up state and can use it for sidebar status. Children do not own the main session's turn box. Buttons route through app handlers and current availability checks; rendering a suggested `commit` or `merge` is not itself authorization to execute it.

Sources: [H2] around 30,000–41,000; [F4]; [F6].

## 14. Subagents, background tasks, and separate sessions

### Runtime task messages

The checked public declarations provide the following useful fields. Optional fields must remain optional in storage and rendering.

| Message | Relevant fields |
| --- | --- |
| `task_started` | `task_id`, `run_id`, `tool_use_id`, description, subagent type, background flag, spawn depth, parent task ID, task type, workflow name, prompt, transcript/activity suppression flags, UUID and session ID |
| `task_progress` | Task identity, usage such as tokens/tool uses/duration, last tool name, optional summary |
| `task_updated` | Patch fields including status, description, end time, total paused time, error and background flag |
| `task_notification` | Final status, summary, output file, optional usage, resource links, reason and suppression flags |

The `0.3.309` task-update status declaration includes `pending`, `running`, `completed`, `failed`, `killed`, and `paused`. Notification status uses `completed`, `failed`, or `stopped`. These are different unions; do not flatten them by assuming identical strings.

The notification contract can also identify `worker_restart` as a reason. Keep the supplied reason when presenting a stopped/failed task; a worker lifecycle event should not be rewritten as a deliberate user cancellation.

### Task identity across resume

A resumed task can keep its `task_id` and receive a new `run_id`. Late events from an earlier run must not overwrite the active run. The declaration documents ordering comparisons for run IDs; treat them as the documented opaque ordering values rather than parsing an invented timestamp format. Older runtimes may omit them and require a compatibility fallback.

`parent_task_id` describes the runtime task tree. `parent_tool_use_id` attributes transcript messages to a spawning tool call. A task's `tool_use_id` links it to that invocation. These are related but distinct. If a parent has not arrived, preserve the child as an unresolved/root-visible task rather than dropping it.

### Background membership is a snapshot, not a patch

`background_tasks_changed.tasks` is the authoritative complete set of live background tasks. Replace the set, including when the payload is empty. Reset it for a new runtime process. Initialization/reinitialization can supply the current set, including an empty set. Older runtimes may lack this event.

A task outcome is emitted before removal from the background set according to the checked contract; otherwise avoid imposing extra ordering assumptions. A completed foreground turn does not imply an empty background set.

The host maintains active background membership separately from foreground state and updates sidebar/session summaries from it.

### Suppression and liveness

- `skip_transcript` means the task should not appear as ordinary inline transcript activity in the relevant presentation.
- `ambient` work should not keep the session's active-work spinner on merely because a watcher remains alive.
- A tasks panel can still expose separately managed tasks where appropriate.
- A heartbeat, elapsed timer, or progress summary is not a completion event.

### Nested text and progress summaries

`forwardSubagentText: true` forwards fuller completed subagent content with parent attribution. The default is false; the ordinary default forwarding is sufficient for tool-call/result activity but not a complete nested prose transcript. The current documented raw partial-stream path is main-agent scoped, so this option must not be sold as guaranteed token-by-token nested streaming. Support also varies by runtime version and nested/forked execution path.

`agentProgressSummaries: true` enables periodic model-generated progress summaries for applicable foreground/background agents. The documented cadence is approximately 30 seconds and uses additional model work in a forked context. Background MCP operations can have server-provided summaries independently of that flag. These sources should not be conflated.

Neither option was established as explicitly enabled in the normal Code host launch path described above. `forwardNestedSubagentToolCalls` was seen internally but is not a supported public option established by this handoff. [Subagents documentation](https://code.claude.com/docs/en/agent-sdk/subagents)

### Background outputs

An initial tool result may only acknowledge that work was launched. The actual output can arrive with the later task notification. Join it to the originating call using explicit IDs. Resource links in task notifications are important for final file/output cards; the checked declaration bounds this resource-link payload to 50 links and 64 KiB. This notification field is distinct from arbitrary server `_meta` and the initial launch acknowledgement. Do not assume every result is final when first received.

### Interruption and individual stop controls

The public `perTaskStopAffordance` declaration is significant. For an open-input query, advertising individual task-stop UI changes how session interruption handles background work. Without an independent way to stop it, the runtime takes a more conservative stop behavior. A closed-input/one-shot query still stops background work under the documented interrupt behavior. The first attached client's capability wins for a multi-client session.

Only declare this capability if Ferrite actually provides working task-level stop controls. A stop button must target the correct task/run and transition through stopping to its acknowledged outcome; the control cannot merely hide the row.

### Nested transcript presentation

The nested parser handles launching, launch failure, a background handback, resumed runs, pending tools, final responses and usage. It can merge adjacent assistant entries and compatible tool groups, attach preceding thinking, and mark pending calls interrupted when the owning work ends. The main transcript and each agent's nested transcript need separate ownership.

### App side sessions are a third mechanism

Desktop also coordinates separate app sessions through session-start/proposal tools, linked plan rows, session families, completion notes, and parent wakeups. This is broader than an SDK subagent.

The supplied host includes waiting on foreground children, user-wait detection, a short settling interval for permission-card state, and bounded automatic wake/notification behavior. Values encountered include a 10-second settling check, a cap of 10 automatic wakes since human input, and a queue bounded to 32 notes. These are implementation values for that coordination path, not SDK requirements or recommended universal defaults.

It also handles lost/archived/deleted parents and pending session startup. A separate app session can have its own workspace and permissions. Do not represent every child session as just another `parent_tool_use_id`, or equate the runtime background-task set with the app's session list.

Sources: [D1], [D3], [H1] including side-session handling around 581,000–584,000; [F1], [F3], [F6].

## 15. Questions, permission cards, and user dialogs

### Permission brokerage

The SDK's `canUseTool` callback is a broker for requests routed to the application. It is not a notification for every tool execution. Tools already allowed by policy can execute without visiting this callback. If Ferrite needs an observation stream for every call, use the runtime messages and appropriate hooks rather than assuming the permission callback is that stream.

Keep these controls distinct:

- `tools` selects the built-in tools exposed to the agent.
- `allowedTools` affects which tools are allowed without the ordinary approval path; it is not a complete availability whitelist.
- `disallowedTools` denies tools through the relevant runtime policy.
- `permissionMode`, managed settings, hooks and runtime rules also affect the effective decision.
- Custom MCP registration adds tools; availability and permission are separate questions.

The request context can contain an abort signal, tool-call identity, agent identity, suggestions and a reason. Retain those associations through the UI. A permission decision must resolve the pending request exactly once; cancellation, process replacement and session closure must dispose the old pending request. A late click cannot approve a new tool that happens to look similar.

The Desktop broker clears pending state and rejects/settles callbacks when the owning runtime closes. The transcript can separately receive `permission_denied`, sometimes before the corresponding tool row exists, so denial state is retained by call ID.

### `AskUserQuestion`

The checked schema allows one to four questions. A question has the full question text, a short header, two to four options, and a multi-select flag. Options have labels, descriptions and, where supported, previews. Schema guidance includes a header of at most 12 characters and labels of roughly one to five words. The UI provides a free-text alternative; the model should not add a duplicate “Other” option.

The SDK's user-input flow uses the permission callback/control response to return updated question input containing the user's answers. Preserve the full original request plus the answer data rather than returning an unrelated string. Rendering the question does not mean it has been answered.

Keep selection, submission, cancellation and runtime acknowledgement separate. The active state should communicate “waiting for you” while the request is pending. Multi-select, free text and optional rich previews require their own validation. The host's HTML preview configuration is conditional; plain text must remain a supported fallback.

### User dialogs are a separate handshake

`onUserDialog` is not an alias for `AskUserQuestion` or `canUseTool`. The checked SDK also requires `supportedDialogKinds`:

- Omitting the supported-kind declaration means the runtime does not emit those dialogs even when a callback exists.
- A nonempty declaration without the callback is rejected at option intake.
- Only declare kinds the UI can actually display and answer.
- The first attached client's declaration controls the multi-client runtime behavior; worker-restart persistence has its own compatibility rules.
- For an unknown dialog delivered to the callback, the public contract calls for `{ behavior: "cancelled" }`, not an invented affirmative response.

Distinguish callback cancellation from the multi-client case where another attached client can own a request. Do not fabricate answers because a surface lacks the necessary UI.

Sources: [H1], [D1], [D2]. Public references: [user input](https://code.claude.com/docs/en/agent-sdk/user-input), [permissions](https://code.claude.com/docs/en/agent-sdk/permissions).

## 16. Review findings and human comments

### Structured findings

`ReportFindings` is a structured reporting tool in the checked public tool declarations. Its presence in the declarations does not guarantee availability in every session.

```typescript
type ReportFindingsInput = {
  level?: "low" | "medium" | "high" | "xhigh" | "max";
  findings: Array<{
    file: string;
    line?: number;
    summary: string;
    short_summary?: string;
    failure_scenario: string;
    category?: string;
    verdict?: "CONFIRMED" | "PLAUSIBLE";
    outcome?: "fixed" | "skipped" | "no_change_needed";
  }>;
};
```

The schema permits at most 32 findings, ordered by severity, with an empty list when there are none. Paths are repository-relative, line numbers are one-based, and compact labels target 60 characters or fewer. `verdict` represents verification status when that pass occurred. `outcome` is for re-reporting after fixes; it should not be fabricated on an initial finding.

The renderer treats successful, settled findings calls as findings UI. It groups findings by file and computes compact unambiguous path labels. Results that include fix outcomes support a different follow-up presentation from the initial review. A failed findings tool must remain a visible failure rather than silently becoming a successful findings card.

### Human comments are app data

Human review comments are a different entity from model findings. The app owns their anchors, threads, storage and submission. The SDK can consume the resulting conversation input; it does not automatically supply Desktop's comment editor.

Anchor types encountered include:

- A file path, line/span and contextual snippet.
- A selected text range, with exact text and surrounding prefix/suffix context.
- A rendered HTML element, with a selector, description and visible text.
- A highlighted selection from a rendered HTML preview.

Keep author/source, replies, current resolution/version context, and target identity. A DOM-selected string can be generated dynamically and may not appear literally in the source file. A comment about a rendered element should not be turned into a guessed source-line edit.

### Serialization into agent input

The shared implementation serializes comments with structured delimiters including:

```text
<claudeai_review_comments>
<review_group container="...">
<review_comment ...>
<selected_text> ... </selected_text>
<surrounding_context> ... </surrounding_context>
<request> ... </request>
<snippet> ... </snippet>
<thread ...> ... </thread>
<element_selector> ... </element_selector>
<element_description> ... </element_description>
<element_text> ... </element_text>
</review_comment>
</review_group>
</claudeai_review_comments>
```

This is a vocabulary illustration, not a complete serialized record or an SDK wire schema. File/line attributes, replies and target-specific content are populated according to the anchor type. Comments are grouped by target/container, and the thread is supplied as a whole because later replies can change the request.

The serializer distinguishes the user's own request from other authors/AI review text and captured page content. Rendered-page strings are treated as untrusted data. Preserve that boundary when building Ferrite's input: selectors, snippets and third-party replies must not silently become application instructions.

Proposal: implement comments as app-owned typed records and a deterministic input serializer. Preserve exact selected text and usable context, send the complete relevant thread, and record which comments were submitted to which turn. The exact delimiter spelling is optional unless interoperating with an existing consumer that requires it.

Sources: [D2] `ReportFindingsInput`; [F2] findings presentation; [F8] serialization around 358,644 onward.

## 17. Files, artifacts, drafts, widgets, and messages to the user

The following inventory preserves the additional app capabilities found. Many belong to Cowork or a shared renderer rather than the normal Code runtime. Definition, registration, session availability and visible support are four separate facts.

### Cowork tool inventory

The `cowork` tool namespace contains definitions for:

| Tool | Purpose established in the implementation |
| --- | --- |
| `request_cowork_directory` | Requests access to an additional directory through the app |
| `allow_cowork_file_delete` | Presents an app-managed file-deletion permission flow |
| `present_files` | Promotes/presents output files as user-facing file references/cards |
| `import_files` | Copies selected external/connected files into the accessible workspace under app policy |
| `launch_code_session` | Starts a separate Code session from an applicable Cowork flow |
| `save_skill` | Saves a skill through an app-managed operation |
| `propose_skills` | Offers skills through a dedicated proposal surface |
| `create_artifact` | Creates an app-managed artifact from supported content/files |
| `update_artifact` | Updates an existing app artifact |
| `list_artifacts` | Lists the relevant app artifacts |
| `verify_artifact` | Checks rendered artifact behavior/diagnostics through the app bridge |
| `screenshot_artifact` | Obtains a screenshot of the rendered artifact |
| `screenshot_file_preview` | Obtains a screenshot from the app's file preview |
| `read_widget_context` | Reads app-maintained context for a rendered widget |
| `writing_draft_v0` | Creates an editable writing-draft card |
| `send_user_message` | Presents exact user-facing text between other agent actions, when enabled |

This is an inventory of definitions, not a statement that all tools are registered together or available to this user's current session.

### File cards and path handling

`present_files` maps runtime/VM paths into files the host can present. It can promote scratchpad outputs into the outputs directory before returning references. Promotion failures matter: a preview may remain possible while opening the file directly on the host is unavailable. A path mentioned by a model is not sufficient proof that the app can open or download it.

`import_files` uses explicit selected/connected access, copies files into the working environment, and checks organization/file policies. Its failure categories distinguish unavailable or network paths, per-file and total-size limits, and copy failures. Directory access and deletion tools invoke actual dialogs; their presence is not merely a prompt convention.

Proposal: define a file-output entity containing runtime path, host/resolved location where applicable, name, MIME/type, size when known, availability and preview/download capabilities. Derive a card from a validated output reference. Keep workspace files, generated outputs and published artifacts distinct even if the UI makes them look related.

### Artifact rendering and verification

The app artifact tools coordinate files/assets, an artifact registry, rendered views and editing. Verification can return console/bridge results. Screenshot callbacks concern the actual rendered panel rather than a model's claim that the artifact looks correct. Widget context is app-owned state that can be returned to the agent for an interactive loop.

Implementing a comparable feature requires a renderer, asset resolution, lifecycle/version state, tool handlers and an interaction bridge. A specialized tool card alone does not supply those capabilities.

### Public `ArtifactInput` is a different layer

The checked SDK tool declarations contain an `ArtifactInput` with actions:

```text
publish, list, read, list_types, watch, unwatch, status,
upload_asset, list_assets, read_asset, delete_asset, pin, unpin
```

For publishing, the checked shape uses `file_path`; it explicitly does not use an inline-content parameter. Other fields include existing artifact URL, title, description, label, icon, listing limit/scope, asset identifiers and action-specific options. Updating an existing artifact and creating a new one are distinct operations.

The declaration itself includes session-specific restrictions on watch behavior. A schema listing `watch` does not prove that the session supports live subscriptions. Likewise, the ability to read a published artifact can differ for owned and shared content. The backing service, authorization, tool availability and renderer are not granted by importing a TypeScript type.

### Writing drafts

`writing_draft_v0` takes a Markdown body and can accept a `scenario_summary`. The summary is not itself rendered as the draft card's visible body in the observed path, so surrounding assistant prose still matters. The handler acknowledgement is not the entire UI: the frontend recognizes the tool call and renders an editable card.

An implementation should avoid duplicating the complete draft in both the card and ordinary prose. Drafting and sending/publishing remain separate actions. The app tool's prompt describes staging and user review; Ferrite should apply the user's actual authorization rather than adding redundant approval steps to an already authorized action.

### `send_user_message`: exact availability correction

The Cowork definition accepts user-facing message text and returns an acknowledgement; the renderer recognizes the call and presents the text. This is useful for explicit messages between tool operations. It is not the required mechanism for every normal reply or the final answer.

Registration depends on `hasSendUserMessage`. The config fallback is `{ enabled: false }`. Gate/config key `3045399524` can provide boolean or session-type-dependent enablement, with additional identifier conditions. `alwaysLoad` defaults false in this path, and the prompt can be overridden. This explains why a definition can exist while the agent correctly reports that the tool is unavailable.

The supplied screenshot establishes only that:

- The session created/presented `demo.md` as a file card; the accompanying reply attributed file delivery to `SendUserFile`.
- Claude said it did not have a `send_user_message` tool in that session.
- It described `PushNotification` as a device alert and `SendMessage` as communication to other agents/sessions.
- Ordinary replies to the user remained ordinary chat text.

Native `SendUserMessage` and Cowork `mcp__cowork__send_user_message` naming must not be collapsed into proof of identical registration. Use the actual session tool list and supported renderer policy.

### Other dedicated presentation categories

The Code/shared registries also contain dedicated presentation for connector suggestions, credential requests, skill/plugin installation or proposals, MCP apps, verification-code entry, workflows, design output and session proposals. Their classification was established; a complete backend contract for every such feature was not. A generic tool fallback should remain available until Ferrite intentionally implements each interaction.

Sources: [H4], [H5], [H6] send-message configuration around 268,962 and registration data around 275,024; [F2], [F4], [F5], [D2].

## 18. Shared-renderer grouping and collapsible narration

These findings explain the broader app behavior the user asked about. They must remain distinguished from the Code-specific path in sections 9–10.

### Narration metadata and local classification

The shared renderer understands `narration_fold` with a `narration_fold_hold` fallback. Values such as folded/hidden versus shown determine presentation; an undecided state can defer the choice while more context arrives. Explicit send-to-user content is protected from ordinary narration folding. Citations and other user-meaningful structure also affect eligibility.

There is a local logistic classifier with fixed feature normalization/weights and a score threshold of `0.10735388526449063`. This classifier itself does not make an additional model API request. Its features include:

- Character, word and newline counts.
- Text position, including first-text status.
- Number of tools before the text and since previous text.
- Whether thinking preceded it and tools follow it.
- Ending punctuation such as colon, ellipsis or question mark.
- Beginning/continuation wording.
- Bullets, headings, code fences, links and table-like content.

The presence of local classification does not mean every collapsed activity label is machine-learned. Other summaries come from runtime/model/server fields or deterministic tool descriptions.

### Group-level policy

Shared grouping code supports server-supplied collapsibility, classifier decisions and heuristics with length/run-size limits. It can hold an undecided text block to avoid immediately showing then hiding it. Citations and meaningful structured text are protected in relevant branches. A completed run that would otherwise lose all its prose can retain a representative/largest text block.

Display-group identity can use `display_group.run.anchor_id`, falling back to `display_group.id`; folded-text run anchors associate text with the intended group. These fields are app/shared-presentation metadata, not universal public SDK message fields.

Group captions consider supplied summaries, localization/language validity, failures and settled/cancelled status. A worker caption is not automatically the same as a turn recap or a tool summary. Failure state can override a flattering group caption.

### Latest-two activity rows

In the shared timeline path, eligible items include thinking, tool use/results and token-budget information. Results/token-budget items do not necessarily create separate visible rows. Special widgets can be excluded from ordinary grouping.

Established behavior in that path includes:

- A single eligible item can render as an ordinary row.
- A collapsed group shows the latest two visible rows, with a count/control for older activity.
- An observed caller keeps certain all-thinking/Task groups expanded.
- Artifact/integration-specific UI flags can alter treatment.
- Open/expanded item state is tracked separately from the underlying messages.
- Height/opacity transitions use roughly 0.2-second timing in the observed presentation.

This is **not** established as the current Code transcript's universal last-two rule.

### Code's currently inactive folding branch

With `foldNarration: "never"`, the Code folding function returns no folded/held text. If another configuration activates it, the available branch includes these guards:

- Feature caps such as 23 preceding tools and text index 2 for the classifier input.
- Short-prose bounds around 20 words and 150 characters.
- Single-line eligibility and checks against questions, bullets, code fences and headings.
- Guards for multiple sentences, file links and issue references.
- Handling of tiny partial prefixes of approximately three words while streaming.
- Previously shown-text tracking to avoid unstable reclassification.
- Final-tail protection and deferred/released decisions as more context arrives.

These constants describe the supplied branch and should not become an unexplained collection of magic numbers in Ferrite. If Ferrite introduces narration folding, make it an explicit product mode, preserve user-facing content and citations, and validate it with representative transcripts. Deterministic rules may be sufficient initially. There is no baseline SDK `isNarration` field that makes this automatic.

Sources: [F9] around 179,165–180,667; [F10], [F11], [F13], [F14]; [F2] configuration/folding around 1,388,258–1,405,545.

## 19. SDK integration checks for Ferrite

These are concrete checks for the implementation agent. They are not confirmed problems in Ferrite.

### Runtime configuration and lifetime

1. Record the SDK package version, selected CLI executable/version, backend kind and effective session ID separately. The Desktop app version and a separately installed `claude --version` do not identify the running session's runtime by themselves.
2. Select the intended `claude_code` prompt preset explicitly and append only instructions describing tools Ferrite actually provides. Keep a custom system-prompt override intentional.
3. Resolve `cwd`, worktree, additional directories, settings sources and actual file access consistently. Confirm that project instructions are loaded when intended. In the checked release, `settingSources: []` means isolation; omission loads the available default sources.
4. Merge the required inherited environment when supplying `env`. Do not accidentally remove PATH, runtime configuration or required authentication/launcher variables.
5. Use an open input stream for an interactive, multi-turn session where appropriate. Closing the input is a lifecycle decision and can change how background work and interruption behave. [Streaming versus single-message input](https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode)
6. Resume by the specific runtime session ID. A generic “continue the latest conversation” operation is insufficient for multiple concurrent app tabs.
7. Keep persistence enabled if later resume is a product requirement. `persistSession: false` removes that expectation.
8. Protect session replacement with a runtime generation/ownership check. A superseded iterator's late event, failure or cleanup must not overwrite the replacement runtime's state.
9. Separate a turn `result`, iterator exhaustion, user interruption, connection loss and process exit. A stream ending without the expected result needs an incomplete/error path.

### Event handling

10. Enable `includePartialMessages` if the UI needs live text/tool-input progress. Reconcile partials into completed assistant messages by API message identity.
11. Do not mark a tool successful at `content_block_stop`, `message_stop`, or the end of argument JSON. Wait for the matching outcome; background launches need their task lifecycle too.
12. Join parallel calls/results by tool-call ID. Results can arrive in a different order from calls.
13. Interpret outer `user` messages by their content and metadata. Tool results and replay/meta input must not appear as fresh human chat bubbles.
14. Keep `parent_tool_use_id`, agent ID and task hierarchy. Child prose and todo updates must not leak into the main assistant message/checklist.
15. Use event/message identity and ordering for replay; do not deduplicate by text or array index. Reconcile optimistic user submission with authoritative echoes.
16. Merge `task_updated.patch`; replace `background_tasks_changed.tasks`. These opposite update semantics need separate reducer operations.
17. Track `run_id` when present, reject stale-run updates, and support older events without it. Include `paused` if the pinned runtime supports it.
18. Apply `skip_transcript` and `ambient` separately from execution state. An ambient watcher must not leave the main activity spinner running forever.
19. Preserve output links that arrive on task notification. The launch acknowledgement can precede the actual output by a long time.
20. Keep tool-use summaries, task-progress summaries, group captions, thinking estimates, model prose and wrap-up recap as distinct inputs to presentation.
21. Treat unknown event types and optional fields compatibly. Log diagnostically or preserve them as data; avoid showing raw event objects as ordinary prose.

### Tool availability and user interactions

22. Separate tool selection, registration and permission. `allowedTools` is not a substitute for a restricted `tools` selection.
23. Use the actual main-session capability/tool information. A schema in an installed package or a deferred tool-search result is not proof that a particular tool is loaded and callable in this session.
24. If native todos are desired, account for the release/model-specific opt-in behavior. If a custom plan is desired, register it and describe it in the prompt. Decide which owns the visible plan.
25. Do not rely on `canUseTool` to observe every execution. Use the event stream/hooks for observation, and the callback for the approval requests it receives.
26. Associate every question, permission and dialog with the owning runtime and tool/control request. Honor cancellation and reject stale responses.
27. Declare only supported dialog kinds, provide the callback, and implement the documented unsupported-dialog cancellation response.
28. Set `perTaskStopAffordance` only when working individual stop controls exist. Test open-input and closed-input behavior separately.
29. If nested prose is needed, consider the public `forwardSubagentText` option and test the chosen runtime. Do not promise nested token streaming solely because this flag is enabled.
30. If progress summaries are desired, deliberately enable `agentProgressSummaries` and account for its extra model work. Do not invent a CLI argument from the SDK option name.
31. Do not copy the internal `forwardNestedSubagentToolCalls` option into a public integration without supported documentation for the chosen release.
32. Treat `_meta['claude/endTurn']`, app-only metadata and Desktop-specific tool aliases as conditional integrations. They are not generic MCP guarantees.
33. Derive available Git/PR actions from current app state and validate again on click. Suggested labels are not executable commands.
34. Supply a generic tool renderer for unknown/custom tools and a visible error path even for tools normally lifted into specialized UI.

### Additional public options relevant to these features

The checked `0.3.309` declarations include several related mechanisms worth preserving in the handoff:

- **Application-composed input:** `verbatimPrompts` sets `client_composed: true` for submitted input, avoiding normal slash-command and mention expansion. Per-message `client_composed` allows narrower control. In the documented current CLI behavior, this also skips the turn-start attachment pass, including some context normally attached with the prompt; much of that context can arrive after the first tool call instead. It is therefore not a consequence-free string-escaping switch. The declaration requires CLI 2.1.248 or later for the field to take effect.
- **Skills:** the public `skills` setting can enable all discovered skills or a named selection. Omitting it is not the same as “skills off”; CLI defaults still apply. The selection is a context/tool filter, not filesystem isolation. This matters if Ferrite wants Desktop-like skill discovery while keeping its advertised capabilities accurate.
- **External transcript storage:** alpha `sessionStore` mirrors local transcript persistence; it is not compatible with `persistSession: false`. The declaration also contains `sessionStoreFlush` and a load timeout. These are optional supported-release mechanisms, not proof that Desktop uses that adapter for its own metadata.
- **Precise resume/fork points:** `resumeSessionAt` accepts a chain-entry UUID, which is not necessarily the last assistant-message UUID. End-turn tools and transcript-only appends can require a later entry. `resumeDropsTurn` can validate that a truncating resume discards only the intended prompt's turn. A rejected validation must be surfaced, not silently replaced with an unsafe truncation or an unrelated new session.
- **Hooks:** `includeHookEvents` controls the broader hook lifecycle stream; setup/session-start events have special behavior in the checked declaration. A hook spinner should be based on the events actually provided, not presumed from configuration.
- **Thinking progress:** `thinking_tokens` supplies estimates useful for progress UI. These are not the authoritative billed output-token count. Keep usage/cost reporting based on the appropriate result/usage data.

Sources: [D1], [D3], [H1], [H3]. The [TypeScript SDK reference](https://code.claude.com/docs/en/agent-sdk/typescript) is a useful entry point; pin implementation decisions to the actual installed release.

## 20. Workspace changes, Git actions, and transcript performance

### File changes and Git/PR state

The host supports `system/code_change_published` and `system/vcs_state_changed` on the live runtime path. It validates relevant fields, binds a published code-change URL, tracks written branches, and invalidates VCS state. Recognized shell/write-tool outcomes can also trigger file/Git refreshes. These are host side effects of events, not extra chat text.

The app maintains diff/VCS caches and refreshes them when relevant work changes files or publishes code. Background work can change these caches after the main turn is complete. An accurate transcript therefore needs an output/change projection that is not limited to the currently streaming assistant bubble.

The PR workflow distinguishes a new versus existing PR, base branch, uncommitted work, push state and draft versus ready status. App-generated action input supplies workflow context. A `<pr-created>URL</pr-created>` marker and supported GitHub URL parsing are compatibility paths for associating a created PR with the app card; newer structured publication events provide another route. A URL in prose must still be validated before being treated as an actionable PR entity.

The meaningful requirement is that the action handler has current repository/PR state and the user's selected intent. A tool card or button render must not execute the action automatically.

File checkpointing enables a separate rewind mechanism. It does not itself provide a visual diff, restore an entire Git branch, or isolate a conversation fork's working directory. Conversation rewind, file rewind and worktree management require explicit coordination.

### Scrolling and history presentation

The Code transcript composition separates committed/base entries from a live or draining tail. It can preserve stable base rows while the tail updates. One shared-renderer bridge passes `keepAllRowsMounted: true`; do not generalize this into a claim that the entire application always virtualizes, or never virtualizes, transcripts.

Other established behavior includes:

- An earlier-history pager, with loading/failure state distinct from the live reply loader.
- Scroll-position restoration keyed by session.
- Preserving position when older history is prepended.
- Explicit follow-new-output behavior and proximity-to-bottom checks; one encountered path uses a 100-pixel threshold.
- Automatic earlier-history loading for a thin viewport, stopping on error or no growth.
- Measured rows, stable entry IDs and cached parsed entries.
- A distinct live-text store/provider rather than rebuilding every historical row on each token.

Proposal: keep user expansion, selection, focused controls and scroll position in presentation state keyed by stable identities. Do not derive them anew from the current token array. Avoid auto-scrolling away from a comment, permission card or historical message the user is reading.

### Scheduling and updates

The host's visible/background batching and the renderer's commit timer are separate scheduling stages. Both must flush around structural boundaries. App metadata updates can batch at microtask boundaries without delaying a tool result into the wrong activity group.

Avoid tying sidebar, todo, nested-agent and transcript re-rendering to one monolithic “new token” state object. The supplied implementation's separate stores/caches illustrate why these projections have different update rates.

Sources: [H1] VCS parser/handler around 528,329–529,800 and related invalidation paths; [F1], [F3], [F5], [F6], [F12].

## 21. Proposed Ferrite implementation shape

This section is a proposal. It should be adapted to Ferrite's existing architecture by the authorized implementation agent, without introducing an unnecessary new framework.

### Three modules with clear ownership

| Module | Owns | Small outward interface |
| --- | --- | --- |
| `SessionRuntime` | SDK options, query lifetime, input queue, session IDs, process generations, event normalization, task state, pending control requests, persistence/resume | Start/resume, send input, interrupt/stop task, answer request, subscribe to a typed session snapshot |
| `TranscriptPresentation` | Partial-to-complete reconciliation, stable entries, turn grouping, tool policy, summaries, plan/todo projection, agent nesting, history merge | Apply normalized events/history and read ordered presentation entries plus derived state |
| `AppInteractions` | Files/artifacts, comments, question forms, validated next actions, navigation, tool registration and app-specific handlers | Execute a typed user action or tool request and return a typed outcome |

The important boundary is ownership of complexity. UI components should render already interpreted state and dispatch typed actions. They should not each rediscover whether a tool finished or whether a background task belongs to the active turn.

### Canonical state and projections

Keep one authoritative execution model with separate projections for the main transcript, each agent transcript, the checklist, background tasks, files, findings/comments, sidebar and next-step box. Persist raw/normalized events where useful for replay and diagnosis, with materialized snapshots if the existing architecture benefits from them. This is a Ferrite recommendation; the Desktop implementation described here combines runtime transcript files, app metadata and renderer stores rather than proving one pure event-sourcing architecture.

Useful invariants:

- A tool call has one identity, regardless of how many views mention it.
- A streamed message and its completed message have one visible representation.
- A task can outlive a turn and can have multiple runs.
- Main and child agents have separate content/plan ownership.
- Waiting for the user is an explicit state with a specific pending request.
- A visible summary never overrides a known failure.
- Every optimistic operation has an acknowledgement/reconciliation path and an explicit failure policy.
- Presentation metadata is optional; absence falls back to a correct generic view.

### A tool presentation registry

Use an explicit registry that can describe:

- Matching tool names/aliases and supported versions.
- Input/result parsing and validation.
- Active/completed/error label builders.
- Grouping policy: ordinary run, dedicated run, standalone, promoted output, or state-only.
- Details renderer and available navigation targets.
- Whether the tool updates a plan, creates an output, opens a user interaction, or starts another task/session.
- Failure fallback when specialized content is unavailable or invalid.

Keep this registry data-driven where it simplifies the code. Do not build a universal plugin framework merely to replace a small switch. Unknown tools should still show their name, state and accessible details.

### Implementation sequence

1. **Execution correctness:** typed event ingestion, identities, process ownership, partial/completed reconciliation, tool-result joining, errors, interruption and resume/history.
2. **Activity presentation:** stable rows, Code-style grouping, tool labels/details, explicit waiting states, thinking/progress and reliable scrolling.
3. **Plans and interactions:** choose the checklist strategy, implement question/permission/dialog brokerage, add validated wrap-up/next actions if in scope.
4. **Agents and background work:** nested ownership, live-task snapshots, task runs, summaries, output delivery and real individual stop controls.
5. **Outputs and review:** file cards/previews, findings, comments/anchors/threads, then app artifacts/drafts/widgets as their backends become available.
6. **Optional presentation refinement:** explicit shared-style folding modes, additional specialized cards and measured performance improvements.

Do not make the semantic foundation depend on optional narration classification or a particular animation. Conversely, do not declare parity on the basis of visually similar rows if event replay, permissions or background work remain wrong.

### Capability contract

At session start and on supported capability updates, record what the runtime, app host and current viewing surface can actually do. A useful capability record distinguishes:

- Registered/available tools and deferred discovery.
- Supported control/dialog kinds.
- Optional event families and version-dependent fields.
- Available file/preview/artifact backends.
- UI affordances such as individual task stop and next-step actions.
- Whether the viewer is local, remote, or missing an app-specific card surface.

Drive tool prompts, registration and visible affordances from the same capability decisions. This is how the agent's claimed abilities remain aligned with what the user can actually use.

## 22. Acceptance fixtures for the implementation agent

These are proposed tests, not tests already run against Ferrite. Use compact real or synthetic event fixtures that exercise observable behavior. The expected result should validate a meaningful invariant rather than repeat an implementation detail.

| # | Fixture | Required visible/state result |
| --- | --- | --- |
| 1 | Text deltas followed by the complete assistant message with matching API ID | One message; no duplicate streamed/final copy |
| 2 | Tool input arrives as several incomplete JSON fragments | Stable provisional row; no parse crash or premature execution/success |
| 3 | Tool argument block stops while the tool remains active | Tool remains running until its correlated outcome |
| 4 | Parallel calls A/B return B/A | Each result updates its own row and grouping position |
| 5 | Outer `user` frame contains tool results or replay/meta content | No invented human chat bubble |
| 6 | Tool fails, including a tool normally rendered only as a specialized card | Failure remains visible and is not hidden by a success caption |
| 7 | Permission denial arrives before the matching call row | Denial attaches once the call arrives; no perpetual spinner |
| 8 | Unknown tool or unfamiliar optional event field | Compatible generic presentation; no crash |
| 9 | Old runtime emits after a replacement query starts | Old event/cleanup cannot mutate replacement-owned state |
| 10 | History seed overlaps live messages and an optimistic user echo | Stable order and one copy of each identity |
| 11 | Earlier history loads while the user is scrolled up | Position and expanded rows remain stable |
| 12 | Stream ends without the expected result | Incomplete/error state; no fabricated success |
| 13 | User interrupts a foreground turn | Correct interrupted outcome, pending interactions canceled as appropriate |
| 14 | Background agent remains after a foreground result | Main turn is settled while background work remains separately visible |
| 15 | Open-input interruption with real per-task-stop support, then explicit task stop | Turn interruption and task stop have the runtime's distinct semantics |
| 16 | Closed-input/one-shot interruption | Background handling matches the closed-input contract |
| 17 | `background_tasks_changed` supplies an empty array | All previously live background membership is cleared |
| 18 | `task_updated` contains only a description patch | Existing unrelated task fields are preserved |
| 19 | A resumed task receives a late event from its prior `run_id` | New run remains authoritative |
| 20 | Task begins before its parent is known | Task remains visible/reconcilable and later attaches to its parent |
| 21 | Ambient or transcript-suppressed task stays alive | No inappropriate inline activity or permanent main spinner |
| 22 | Background launch acknowledgement followed by later resource links | Final outputs attach to the original operation |
| 23 | Child text, thinking and todo calls arrive | Nested transcript updates; main prose/checklist is not polluted |
| 24 | Runtime supplies complete child text but no child token deltas | UI accurately shows supported nested updates without promising absent streaming |
| 25 | Tool-use summary, task summary and turn recap arrive near each other | Correct ownership/provenance; no accidental replacement of the final answer |
| 26 | Question with multi-select/free text is submitted, canceled or interrupted | Answers route once to the correct request; canceled/late answers have no new effect |
| 27 | Unsupported dialog or missing supported-kind declaration | Documented fallback/cancellation; no fabricated approval |
| 28 | Two `TaskCreate` calls have identical subjects and different returned IDs | Distinct rows rekey by call identity and retain order |
| 29 | Failed `TaskUpdate` changes an existing item or provisionally adds an unknown item | Defined rollback/removal behavior preserves correct ordering |
| 30 | Failed `TodoWrite` replacement appears in replay | Previous valid list survives |
| 31 | Failed `TaskCreate` | Ferrite's explicitly chosen failure behavior is tested; no unsupported assumption about the Desktop reducer |
| 32 | Native task deletion and a full todo replacement | Deletion/remapping behaves correctly with stable UI identity |
| 33 | Plan replacement includes incremental fields, followed by one-based updates | Replacement precedence and index validation match the chosen contract |
| 34 | Subagent attempts to change the main custom plan | Main-plan ownership remains intact |
| 35 | Compaction/interruption occurs mid-plan with a child still running | Plan is not blindly marked complete or discarded |
| 36 | Recap-only wrap-up versus blocked/question/action wrap-up | Only the applicable next-step UI appears; status still updates |
| 37 | Wrap-up is followed by ordinary final prose | Recap and final answer coexist without unwanted duplication |
| 38 | An offered Git/PR action becomes invalid before click | Revalidation prevents stale action execution and updates the affordance |
| 39 | Reply-style label contains command prefix or control characters | Input validation prevents a hidden command payload |
| 40 | Finding/report tool fails, or re-reports fix outcomes | Failure, initial finding and follow-up outcome remain distinct |
| 41 | Comment has a selected range, later replies and a rendered DOM anchor | Correct target/context/thread reaches the agent as data |
| 42 | File output exists only inside a VM or promotion fails | Available preview/download/open actions match actual accessibility |
| 43 | Artifact schema exists but the tool/backend is unavailable | No false availability claim or nonfunctional artifact action |
| 44 | Hidden session receives rapid deltas, then becomes visible | Batching preserves order and final content; structural events are not stranded |
| 45 | Code mode versus explicitly enabled shared-folding mode | Code's configured no-fold behavior is preserved; last-two grouping is applied only in the intended mode |
| 46 | Long history streams while a card/comment is focused | Focus, selection, expansion and reading position remain stable |
| 47 | Retry, compaction, thinking estimates and usage result arrive | Accurate status transitions; estimated progress is not reported as billed usage |
| 48 | Capability is disabled, omitted or changes after initialization | UI/prompt/registration remain aligned, with a usable generic fallback |
| 49 | A conversation fork shares the existing filesystem | UI does not falsely claim a new isolated worktree or restored files |
| 50 | A truncating resume fails validation because unseen entries would be lost | Error is surfaced without silently discarding the intervening work |

### Definition of done for the receiving agent

For the chosen implementation scope, event replay must produce deterministic semantic state; live and historical views must reconcile; all enabled interaction cards must have working reply/cancel/error paths; background ownership must survive foreground completion; and enabled capabilities must match the actual runtime. Verify the relevant fixtures above, then check the rendered transcript under active streaming, waiting and long-history conditions. Document any remaining unsupported capability explicitly.

## 23. Unresolved details and compatibility boundaries

The following must not be silently promoted into established facts:

- Exact behavior of unseen server-side components and every rollout/account variant.
- Which gated tool set a future or different live session will receive.
- That the separately installed CLI is the executable Desktop selected.
- That a newer SDK field is supported by an older selected CLI.
- That the normal Code path enables the optional subagent text/progress-summary flags.
- That internal SDK options are stable or public.
- That a successful-looking static definition guarantees an enabled working backend.
- That shared-renderer grouping or folding is active in the supplied Code configuration.
- That every optimistic todo operation has identical rollback semantics.
- That a wrap-up always ends the turn or always replaces final prose.
- That an outer event type, array position, message title or task subject is sufficient identity.
- That every app-specific card's complete backend contract was established merely because its renderer category exists.
- That file checkpointing, conversation resume and Git/worktree isolation are interchangeable.
- That any possible Ferrite integration issue listed here is already a verified Ferrite bug.

The desired fidelity is achievable only against a pinned capability set and verified observable behavior. Keep compatibility adapters small and explicit, retain unknown data safely, and prefer supported public extension mechanisms for Ferrite's own tools.

## 24. Ancillary runtime diagnostics and integrity findings

These findings were part of the wider discussion but do not add transcript features:

- The Desktop host contains event logging through `/api/event_logging/v2/batch`, with account/organization attribution in related event data.
- Crash reporting and process/runtime diagnostics are present. Stream-end diagnostics can include timing, session/runtime identity, backend kind and a bounded stderr tail.
- The app has ASAR integrity enforcement enabled; the two app-archive header hashes matched the values recorded in app metadata at the time represented by this handoff.
- The main app/native-helper entitlement information available in this work did not include an Endpoint Security entitlement.
- Native symbols include `proc_listpids` and `proc_pidinfo`. Symbol presence alone does not establish the purpose or timing of every call.
- The available native evidence did not establish a `ptrace`/`P_TRACED` mechanism. The available log evidence did not establish a file-read reporting mechanism. These are bounded negative findings, not proof that no other mechanism exists.
- This information does not support a guarantee about all telemetry or account outcomes.

These points should not influence transcript semantics beyond ordinary diagnostic separation: app logs, crash metadata and stderr belong in diagnostics, while user-visible state comes from the relevant execution events. There is no need to copy Desktop's telemetry service into Ferrite.

Sources: Desktop bundle metadata and native components; [H1] runtime diagnostics and event paths. No account identifiers, credentials, raw diagnostic logs or full app prompts are included in this handoff.

## 25. Source catalog and reproducibility pointers

### Local source references

All source keys below resolve through this temporary directory:

```text
/tmp/claude-desktop-transcript-sources-2026-11-03
```

The directory is a local symlink to the existing source workspace. It is not embedded in the Markdown file and may disappear when temporary storage is cleared. If handing this file to an agent on the same machine, these links are usable while the directory exists. For an agent on another machine, the behavioral descriptions and public versioned declarations remain the portable part of the handoff.

Offsets in this document are approximate **character offsets in decoded UTF-8 text**, not byte offsets or line numbers. Most generated JavaScript files are minified. Prefer a named symbol, event/tool string or nearby structure when locating a behavior; filenames and offsets change between app builds.

| Key | Source file | Main relevance |
| --- | --- | --- |
| [H1] | `app.asar/.vite/build/index.chunk-C3r2jk0E.js` | Code host, SDK options, session lifetime, event routing, background work, persistence, VCS and diagnostics |
| [H2] | `app.asar/.vite/build/index.chunk-BReS2dO4.js` | Turn-plan/wrap-up tools and stream coalescer |
| [H3] | `app.asar/.vite/build/index.chunk-CZxSNxKH.js` | Bundled SDK implementation/plumbing and version marker |
| [H4] | `app.asar/.vite/build/index.chunk-m4oMImJ-.js` | Cowork app-tool definitions, outputs, artifacts, drafts and user messages |
| [H5] | `app.asar/.vite/build/index.chunk-BUZLSRt0.js` | Cowork tool registration and access-related integration |
| [H6] | `app.asar/.vite/build/index.chunk-Bvm2vgWR.js` | Cowork setup, configuration and gated capabilities |
| [F1] | `ion-dist.asar/assets/v1/c06cf64bb-Czlb7fkW.js` | Code transcript store, main parser and live stream buffer |
| [F2] | `ion-dist.asar/assets/v1/c96d752d0-BKceltwO.js` | Code presentation, tool grouping, findings, turn state and folding branch |
| [F3] | `ion-dist.asar/assets/v1/c5da7eb42-D8jtksf3.js` | Capability detection and incremental/nested transcript parsing |
| [F4] | `ion-dist.asar/assets/v1/cb27dab29-DNoIcYzd.js` | Plan/wrap-up parsing and tool classification helpers |
| [F5] | `ion-dist.asar/assets/v1/cb0636f7b-Dis95AbK.js` | Content normalization, tool helpers and PR-related input/link handling |
| [F6] | `ion-dist.asar/assets/v1/c4ea84cb3-CizlIWl6.js` | Plan replay, transcript composition, history and scrolling |
| [F7] | `ion-dist.asar/assets/v1/c28efd0ef-DFNllFV3.js` | Complete native todo reducer |
| [F8] | `ion-dist.asar/assets/v1/shared-147-DedKKxiE.js` | Review/comment anchors and serialization |
| [F9] | `ion-dist.asar/assets/v1/shared-4-DzYLI5T8.js` | Shared narration classifier and metadata |
| [F10] | `ion-dist.asar/assets/v1/cc1aae219-BwKj3o0f.js` | Shared transcript composition |
| [F11] | `ion-dist.asar/assets/v1/c5ac695a3-CVbBnmXP.js` | Shared activity timeline and latest-two grouping |
| [F12] | `ion-dist.asar/assets/v1/shared-8-BeJUm-0j.js` | Frontend session-list/event state |
| [F13] | `ion-dist.asar/assets/v1/c05b96c7f-BE-al_cp.js` | Shared group folding, status and summary rules |
| [F14] | `ion-dist.asar/assets/v1/shared-107-D39s5ReM.js` | Tool metadata and browser/computer action labels |
| [D1] | `public-sdk/0.3.309-sdk.d.ts` | Pinned public SDK types/options/events |
| [D2] | `public-sdk/0.3.309-sdk-tools.d.ts` | Pinned public tool input types |
| [D3] | `public-sdk/0.3.317-sdk.d.ts` | Later published SDK declarations used for comparison |
| [D4] | `public-sdk/0.3.317-sdk-tools.d.ts` | Later published tool declarations used for comparison |

App version metadata is at [Claude Info.plist](/Applications/Claude.app/Contents/Info.plist). The source files above correspond to resources in `/Applications/Claude.app/Contents/Resources/app.asar` and `/Applications/Claude.app/Contents/Resources/ion-dist.asar` for the stated app build. Do not silently apply the same offsets to a future update.

### Public references

- [TypeScript SDK reference](https://code.claude.com/docs/en/agent-sdk/typescript)
- [Custom tools](https://code.claude.com/docs/en/agent-sdk/custom-tools)
- [Todo tracking](https://code.claude.com/docs/en/agent-sdk/todo-tracking)
- [Streaming output](https://code.claude.com/docs/en/agent-sdk/streaming-output)
- [Streaming versus single-message input](https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode)
- [Sessions](https://code.claude.com/docs/en/agent-sdk/sessions)
- [Subagents](https://code.claude.com/docs/en/agent-sdk/subagents)
- [User input](https://code.claude.com/docs/en/agent-sdk/user-input)
- [Permissions](https://code.claude.com/docs/en/agent-sdk/permissions)
- [System-prompt configuration](https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts)
- [Published SDK 0.3.309 types](https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.3.309/sdk.d.ts)
- [Published SDK 0.3.309 tool types](https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.3.309/sdk-tools.d.ts)
- [Published SDK 0.3.317 types](https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.3.317/sdk.d.ts)
- [Published SDK 0.3.317 tool types](https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.3.317/sdk-tools.d.ts)

Live documentation can change. Use the versioned declarations and actual session capabilities to resolve differences rather than assuming that a current documentation page describes every installed runtime.

[H1]: /tmp/claude-desktop-transcript-sources-2026-11-03/app.asar/.vite/build/index.chunk-C3r2jk0E.js
[H2]: /tmp/claude-desktop-transcript-sources-2026-11-03/app.asar/.vite/build/index.chunk-BReS2dO4.js
[H3]: /tmp/claude-desktop-transcript-sources-2026-11-03/app.asar/.vite/build/index.chunk-CZxSNxKH.js
[H4]: /tmp/claude-desktop-transcript-sources-2026-11-03/app.asar/.vite/build/index.chunk-m4oMImJ-.js
[H5]: /tmp/claude-desktop-transcript-sources-2026-11-03/app.asar/.vite/build/index.chunk-BUZLSRt0.js
[H6]: /tmp/claude-desktop-transcript-sources-2026-11-03/app.asar/.vite/build/index.chunk-Bvm2vgWR.js
[F1]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/c06cf64bb-Czlb7fkW.js
[F2]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/c96d752d0-BKceltwO.js
[F3]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/c5da7eb42-D8jtksf3.js
[F4]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/cb27dab29-DNoIcYzd.js
[F5]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/cb0636f7b-Dis95AbK.js
[F6]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/c4ea84cb3-CizlIWl6.js
[F7]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/c28efd0ef-DFNllFV3.js
[F8]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/shared-147-DedKKxiE.js
[F9]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/shared-4-DzYLI5T8.js
[F10]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/cc1aae219-BwKj3o0f.js
[F11]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/c5ac695a3-CVbBnmXP.js
[F12]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/shared-8-BeJUm-0j.js
[F13]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/c05b96c7f-BE-al_cp.js
[F14]: /tmp/claude-desktop-transcript-sources-2026-11-03/ion-dist.asar/assets/v1/shared-107-D39s5ReM.js
[D1]: /tmp/claude-desktop-transcript-sources-2026-11-03/public-sdk/0.3.309-sdk.d.ts
[D2]: /tmp/claude-desktop-transcript-sources-2026-11-03/public-sdk/0.3.309-sdk-tools.d.ts
[D3]: /tmp/claude-desktop-transcript-sources-2026-11-03/public-sdk/0.3.317-sdk.d.ts
[D4]: /tmp/claude-desktop-transcript-sources-2026-11-03/public-sdk/0.3.317-sdk-tools.d.ts
