# Provider options audit: what Ferrite could turn on

Date 2026-11-03. Checked against Claude Code **2.1.317**
(`~/.local/share/claude/versions/2.1.317`) and **codex-cli 0.166.0**. Ferrite's
pins are 2.1.224 and 0.149.1.

This audit goes through the options in
`claude-desktop-transcript-handoff.md` (§6, §14, §15, §19 "Additional public
options"). For each one it records:

- the CLI or `initialize` equivalent of the TypeScript SDK option;
- whether Ferrite uses it now;
- what it would give Ferrite;
- a recommendation.

No paid session was run. The Claude evidence is from `claude --help` and from
strings in the compiled binary. The binary contains a copy of the SDK's
`ProcessTransport`/`Query` code, which shows how each SDK option becomes a
flag, an env var or an `initialize` field. It also contains the CLI's zod
schema for the `initialize` request. The Codex evidence is from
`codex app-server generate-json-schema --experimental`. Where something below
depends on runtime behaviour, it should be confirmed with one capture before
it ships.

## How SDK options reach a stream-json CLI

There are three ways, and the SDK uses all three:

1. **Flags**, some of them hidden. These include `--thinking`,
   `--resume-session-at`, `--resume-drops-turn`, `--rewind-files`,
   `--include-hook-events`, `--allow-dangerously-skip-permissions` and
   `--session-mirror`.
2. **Env vars the SDK sets**. Excerpt from the transport:
   ```js
   if(C)M.CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING="true";            // enableFileCheckpointing
   if(!…"CLAUDE_CODE_SDK_READS_SESSION_STATE")M.CLAUDE_CODE_SDK_READS_SESSION_STATE="1";
   if(Z?.askUserQuestion?.previewFormat)M.CLAUDE_CODE_QUESTION_PREVIEW_FORMAT=Z.askUserQuestion.previewFormat;
   ```
3. **`initialize` fields**. The CLI schema (`subtype:"initialize"`) accepts:
   - `hooks`, `sdkMcpServers`, `sdkMcpServerConfigs`, `sdkMcpServerManifests`
   - `jsonSchema`, `systemPrompt`, `appendSystemPrompt`, `systemPromptSnapshot`
   - `planModeInstructions`, `toolAliases`, `excludeDynamicSections`
   - `agents`, `title`, `skills`
   - `promptSuggestions`, `agentProgressSummaries`, `forwardSubagentText`
   - `supportedDialogKinds`, `perTaskStopAffordance`
   - `plugins`, which needs `--await-initialize`
   - fields marked `@internal`: `rapidFollowupPreempt`,
     `forwardNestedSubagentToolCalls`, `workspaceTrust`,
     `rendersChromeSetupOffer`, `codeRunningMcpTools`

Ferrite spawns `claude -p` itself (`providers/claude.rs:242-300`). Its
initialize sends only `perTaskStopAffordance:true`, plus `sdkMcpServers` when
visuals are on (`claude.rs:391-397`). It therefore gets none of the env vars
the SDK sets. That gap matters more than any flag: see findings 1–3.

## Claude Code

| Option (SDK name) | CLI / initialize equivalent | Ferrite now | What it gives Ferrite | Cost / risk | Rec. |
| --- | --- | --- | --- | --- | --- |
| *(not in handoff)* tool-use summaries | env `CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES=1`. The gate is `gates:{emitToolUseSummaries:i.CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES}` | **Parser only.** `wire.rs:243` `parse_tool_summary` → `SessionEvent::ToolSummary` (commit dc19326b). The env var is never set, so the CLI never emits the frame. | The one-line captions over tool groups that Ferrite has already built | One small side-model call per tool batch (`ol({tools,…})`). Main agent only (`!C.agentId`). | **Use now** |
| *(not in handoff)* session state | env `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1` (public form). `CLAUDE_CODE_SDK_READS_SESSION_STATE=1` is what the SDK sets; it adds `sdk_host_only:true` | **Parser only.** `wire.rs:1387` maps the event to `RunState` (commit 2cb5f2ef). Without the env var it is never emitted: `if(i.CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS)ea(…)else if(i.CLAUDE_CODE_SDK_READS_SESSION_STATE)ea(…)` | The event the schema calls the "authoritative turn-over signal" (`idle` after held-back results and background agents drain). Also `requires_action`. | None found | **Use now** |
| `CLAUDE_CODE_ENABLE_TODO_TOOLS` | env var | Not set | Gives current models in `-p` the `TaskCreate`/`TodoWrite` tools that Ferrite's plan UI reads (`contract_plans.rs`, `transcript/tools.rs`). The gate is `F5(){if(interactive‖todoToolsOptIn)return!0; model unknown‖in kOr→!0; return CLAUDE_CODE_ENABLE_TODO_TOOLS}`. `kOr` lists only models up to `claude-opus-4-7` / `sonnet-4-6` / `haiku-4-5`. So on Opus/Sonnet 5 and Fable in print mode the checklist tools are **absent**. The 2.1.243 fixtures show `TaskCreate…` only because they were captured on haiku-4-5. | The model gets task tools and reminders (`task_reminder`), which changes its behaviour somewhat. Anthropic opted newer models out on purpose. | **Use now, behind a setting defaulting on** if the native checklist is a product goal. Confirm with one capture of `system:init.tools`. |
| `CLAUDE_CODE_ENABLE_TASKS` | env var. `M0(){if(i.CLAUDE_CODE_ENABLE_TASKS===!1)return!1;return!0}` | Not set (defaults to on) | Chooses Task* (on) or TodoWrite (`=false`) when todo tools are present at all | None | **Skip.** Default is right; Ferrite reads both families. |
| `enableFileCheckpointing` | env `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=true`. Print mode checkpoints only with it: `if(ge())return hHo()` → `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING&&!DISABLE`. Rewind is the control request `rewind_files {user_message_id, dry_run?}`, which returns `canRewind, filesChanged, insertions, deletions`. There is also the hidden `--rewind-files <id>` (with `--resume`). | Not set. No rewind code. | Restoring files for edit/rewind. `dry_run` gives the counts for a confirmation card. | File backups under `~/.claude`. Needs user-message UUIDs: Ferrite's `send` sets no `uuid`, though the queued path does (`claude.rs:600`). | **Use when edit/rewind is built**. See `docs/research/edit-rewind.md`. |
| `resumeSessionAt` / `resumeDropsTurn` | hidden `--resume-session-at <chain-uuid>` and `--resume-drops-turn <prompt-uuid>`, print mode with `--resume`. The second "refuses the resume if the discarded range contains anything not attributable to that turn". Also `--fork-session`. | Not used. `--resume` only (`claude.rs:274`). | Truncating conversation history for edit/rewind. `--resume-drops-turn` makes it safe. | A respawn per rewind. A refusal has to be shown as an error, never silently retried. | **Use with edit/rewind** (see `edit-rewind.md`) |
| `agentProgressSummaries` | initialize `agentProgressSummaries:true` → `g$n(!0)`. The SDK always deletes the old env var: `[!1,"CLAUDE_CODE_AGENT_PROGRESS_SUMMARIES"]`. | Not sent. Ferrite already renders `task_progress.summary` (`activity.rs:1395,1423`; `wire.rs:422`). | A live one-line status on each subagent row, about every 30 s | Extra forked model work per running agent | **Use now**, perhaps tied to a "subagent summaries" setting |
| `perTaskStopAffordance` | initialize field | **Used** (`claude.rs:386-392`), wired to `stop_task` | Interrupt spares background agents | — | Keep |
| `onUserDialog` / `supportedDialogKinds` | initialize `supportedDialogKinds:[…]`, then control request `request_user_dialog {dialog_kind, payload, tool_use_id?}`. Kinds in 2.1.317 include `refusal_fallback_prompt`, `enum_prompt`, `cost_threshold`, `autocompact_offer`, `auto_mode_server_fallback`, `resume_return`, `newer_model_offer`, `ultraplan_choice`, and nudges/upsells. | Not declared. The CLI fails closed, so none arrive (`wire.rs:913` would refuse one anyway). | Desktop-like recovery prompts. The useful ones are `refusal_fallback_prompt` (switch model after a refusal) and `enum_prompt`. | Payload shapes are opaque per kind and need a capture each. An undeclared kind must not be answered `cancelled`. | **Later**: `refusal_fallback_prompt` first |
| `includeHookEvents` | `--include-hook-events` (public flag) | Not passed. SessionStart `hook_started/response` arrive anyway; hooks are ignored (gaps doc row 5). | A hook spinner or row ("running pre-commit hook…") | More frames. No UI for them yet. | **Later**, together with hook UI |
| `thinking_tokens` progress | Emitted by default: `system/thinking_tokens {estimated_tokens, estimated_tokens_delta, user_message_uuid?}` | Received, but only becomes `Phase::Thinking` with an empty detail (`wire.rs:393`). The estimate is dropped. | "Thinking · ~1.2k" in the live line, attributable to the send that caused it | Only an estimate. Keep it out of usage and cost. | **Use (cheap)**, parser and pane only |
| `verbatimPrompts` / `client_composed` | `client_composed:true` on a user stdin frame (no flag). Skips `@path` and slash dispatch and the whole turn-start attachment pass (nested CLAUDE.md, skill listings, reminders). | Not used. Ferrite depends on CLI slash dispatch (`claude.rs:200-205`). | Exact text for app-generated prompts | Silently loses context. Breaks `/cmd`. | **Skip.** At most, later, per message for prompts Ferrite generates itself. |
| `skills` | initialize `skills:[names]`. This filters the system-prompt listing, not the filesystem. | Not sent (all skills load). Menu comes from the initialize answer (`skill_mentions.rs`). | Hiding skills per Thread | Mismatch between menu and loaded skills | **Skip** |
| `toolConfig.askUserQuestion.previewFormat` | env `CLAUDE_CODE_QUESTION_PREVIEW_FORMAT=markdown\|html`. Without it the CLI turns previews on only for non-SDK entry points: `else if(!r.startsWith("sdk-")&&…)Ryo("markdown")`. | Not set, so the model is never offered `preview`. `questions.rs:46,134` parses `Choice.preview`, but no pane code renders it. | Side-by-side option previews (code or plan) in question cards | Needs a markdown preview surface. Use `markdown`, not `html`. | **Later**: set it in the same change that renders previews |
| `thinking` (adaptive) | hidden `--thinking enabled\|adaptive\|disabled`, where "enabled (equivalent to adaptive)". `--thinking-display` is separate. | Passes `--thinking-display summarized` only (`claude.rs:254`). | Nothing beyond the model default plus effort | — | **Skip** (effort covers it) |
| `systemPrompt` preset + append | Default = `claude_code` preset. `--append-system-prompt`/`--system-prompt`, or initialize `appendSystemPrompt`/`systemPrompt`. | Default preset. Not appended. (ferrite-projects agents do use `--append-system-prompt`.) | A place to describe Ferrite-only behaviour | Recorded once by `--system-prompt-snapshot` (default on). Easy to over-promise. | **Skip** until Ferrite has instructions the `show_visual` tool description cannot carry |
| `mcpServers` / connectors | `--mcp-config`/`--strict-mcp-config`, or initialize `sdkMcpServers` (+`sdkMcpServerConfigs`, `sdkMcpServerManifests`) | `sdkMcpServers:[visual]` (`claude.rs:396`). User MCP comes from settings. | `sdkMcpServerManifests` would skip the in-process handshake round trips at startup | Small win, more code | **Skip / later** (startup latency only) |
| `rapidFollowupPreempt` | initialize field, `@internal`, "honored only while the rollout flag is on". Emits `system/turn_preempted`. | Not sent | A send that arrives before output preempts the turn | Internal and rollout-gated | **Skip** |
| `sessionStore` | SDK-side adapter. The CLI part is hidden `--session-mirror` (`transcript_mirror` frames). | Not used | External transcript storage | Ferrite has its own store. Alpha. | **Skip** |
| `env` | Process env. The SDK copies `process.env` and adds the vars above. | Inherited env minus marker vars (`spawn.rs:117-160`) plus Thread env. Correct. | — | — | **Keep.** Add the env vars from findings 1–3 in `ClaudeSession::spawn`. |
| `allowDangerouslySkipPermissions` | `--allow-dangerously-skip-permissions` (public) | Not passed. Bypass is offered only if the session launched in it (`claude.rs:438-453`). | Switching to bypass mid-session | Safety. Desktop computes it per session. | **Skip.** At most, later, behind an explicit per-Thread opt-in. |

## Codex app-server

Ferrite already uses the following:

- `initialize.capabilities.experimentalApi`
- `thread/start|resume` with `cwd`, `model`, `approvalPolicy`, `sandbox` and
  visuals
- `turn/start` with `summary:"detailed"`, `effort`, `model` and
  `sandboxPolicy.writableRoots`
- `thread/settings/update {approvalPolicy}`
- the queue, steer and background-terminal terminate requests

It also handles these events:

- `turn/plan/updated`, `item/plan/delta`, `turn/diff/updated`
- reasoning summary/text deltas
- `thread/tokenUsage/updated`, `account/rateLimits/updated`
- `model/rerouted`, `serverRequest/resolved`
- `requestUserInput`, permissions and elicitation requests

| Option | Ferrite now | What it gives | Cost / risk | Rec. |
| --- | --- | --- | --- | --- |
| Reasoning summaries (`turn/start.summary`) | `"detailed"` every turn (`codex.rs:603`) | — | — | Keep |
| Plan updates | Handled (`codex/wire.rs:370`) | — | — | Keep |
| `thread/fork {lastTurnId \| beforeTurnId}`, `thread/revert {beforeTurnId}` → `thread/reverted` | Not used | Edit/rewind of conversation history. **Revert does not restore files**: "only changes persisted conversation history". | Files need Codex's turn diffs or git | **Use with edit/rewind** (see `edit-rewind.md`) |
| `approvalsReviewer: "auto_review"` (thread/start, turn/start, settings/update) + `item/autoApprovalReview/*` | Not used. Modes offered are only `untrusted/on-request/never` (`codex.rs:692`). | A Codex counterpart to Claude's "Auto" mode: a subagent reviews escalations | New item kinds to render | **Later** (good parity item) |
| `AskForApproval.granular {sandbox_approval, rules, mcp_elicitations, …}` | Strings only | Finer policy | More UI | Skip |
| `collaborationMode {mode:"plan"}` (turn/start, settings/update; EXPERIMENTAL) + `collaborationMode/list` | Not used | A Codex Plan mode, matching Claude's `plan` | Experimental field | **Later** |
| `hook/started`, `hook/completed` notifications | Not parsed | Hook rows | — | Later, alongside Claude `--include-hook-events` |
| `thread/compact/start` | Not used | A "Compact now" control | — | Later |
| `review/start {target, delivery}` | Not used | A native `/review` | — | Later |
| `developerInstructions` / `baseInstructions` | Not used | Counterpart to append system prompt | Same as the Claude row | Skip |
| `initialize.capabilities.optOutNotificationMethods` | Not used | Drops unused notifications (realtime etc.) | — | Skip |

## Recommended to adopt now

Each item is a small change, and none needs new UI unless noted.

1. **`CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES=1`** in the Claude spawn env. Without
   it, the tool-summary pipeline shipped in dc19326b never receives a frame
   from a real CLI.
2. **`CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1`**. Without it, the `RunState`
   parser from 2cb5f2ef is dead in real sessions. This is the CLI's
   authoritative idle signal.
3. ~~**`CLAUDE_CODE_ENABLE_TODO_TOOLS=1`**~~ Superseded (2026-11-04): confirmed on
   2.1.318 that Opus 5.5 in print mode gets `TaskCreate…` only with it, but even
   then nothing tells the agent to plan. Ferrite now offers Claude Desktop's own
   answer, `mcp__ferrite__update_plan` with §12's guidance
   (`crates/ferrite-core/src/update_plan.rs`), and leaves the native tools as
   the CLI decides, so there is one plan. Codex's `update_plan` is off unless
   `tools.update_plan.enabled`; Ferrite turns it on per thread with the same
   guidance as `developerInstructions`.
4. **`agentProgressSummaries: true`** in the initialize request
   (`claude.rs:391`). The renderer already exists. Accept the extra model
   calls, or gate it with a setting.
5. **Show the `thinking_tokens` estimate** in the live line. The frame already
   arrives; this is a parser and pane change only.

Then, once edit/rewind lands (`docs/research/edit-rewind.md`):

- `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=true`, `rewind_files`,
  `--resume-session-at` and `--resume-drops-turn`;
- user `uuid`s on every send;
- Codex `thread/fork` and `thread/revert`.

Ordering note: items 1–3 belong together in one `ClaudeSession::spawn` env
block next to `command.envs(config.env…)` (`claude.rs:247`). Apply them before
`config.env`, so a Thread's own env can still override them. Add a capture
fixture at 2.1.317 so the parsers stop being tested only against synthetic
frames.
