# Provider thinking and live status

Researched 2026-09-05. Primary documentation, published SDK types, provider
source, and existing captures only; no model requests or tests run for research.

## Claude

Streaming and thinking visibility are separate controls. Agent SDK
`includePartialMessages: true` / Python `include_partial_messages=True` adds
`stream_event` wrappers alongside completed assistant messages. Ferrite already
passes the CLI equivalent, `--include-partial-messages`.
[SDK streaming](https://code.claude.com/docs/en/agent-sdk/streaming-output).

`thinking.display: "summarized"` exposes readable thinking summaries.
`"omitted"` returns empty thinking text and emits no `thinking_delta`; this is
the default on several newer models, including Opus 4.7/4.8/5. Thinking may
also be absent because it is disabled or the model decides it is unnecessary.
`"updates"` is a separate Messages API beta: it exposes progress updates while
omitting reasoning, requires `thinking-display-updates-2026-08-18`, and is not
one of the inspected Agent SDK/CLI display choices. Empty/redacted blocks and
signatures provide no readable text to reconstruct.
[Thinking display and streaming](https://platform.claude.com/docs/en/build-with-claude/thinking#controlling-thinking-display).

Published Agent SDK 0.2.118 types expose `thinking` as adaptive, enabled
(`budgetTokens?`), or disabled. Adaptive/enabled accept
`display?: 'summarized' | 'omitted'`.
[Published SDK declaration](https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.2.118/sdk.d.ts).
The installed Claude 2.1.261 embedded SDK maps that display field to
`--thinking-display`; its CLI accepts `summarized` and `omitted`. Read-only
inspection of the official **2.1.243** platform package confirms the same flag
and choices, so Ferrite's minimum supported version already supports it.
[Minimum-version package](https://registry.npmjs.org/@anthropic-ai/claude-code-darwin-arm64/2.1.243).

**Implementation conclusion:** request `--thinking-display summarized` inside
the Claude adapter, preserving user thinking mode and effort. Missing display
opt-in is a supported explanation for absent thinking, not a proven diagnosis
of a captured failing live session. The existing
[Haiku capture](../../crates/ferrite-core/tests/fixtures/claude-hello-2.1.243.jsonl)
contains thinking deltas; it does not establish newer-model defaults.

Keep ordinary `text_delta` commentary between tools. A content-block stop
ends streamed content, not tool execution. Completed assistant snapshots
coexist with deltas; blindly appending both duplicates responses. This pass
retains the current delta contract and intentionally does not add snapshot
fallback. A future fallback needs message/block reconciliation: raw
`message_start.message.id` identifies the API message; content events carry
`index`; outer `uuid` identifies an SDK event, not a stable content block.
[Message flow](https://code.claude.com/docs/en/agent-sdk/streaming-output#message-flow),
[raw event shape](https://platform.claude.com/docs/en/build-with-claude/streaming).

For live status, SDK types provide `tool_progress` with `tool_use_id`,
`tool_name`, and `elapsed_time_seconds`; `tool_use_summary` supplies `summary`
and `preceding_tool_use_ids`. These can supplement streamed thinking without
inventing provider text.
[SDK message declarations](https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.2.118/sdk.d.ts).

## Codex

App-server `item/reasoning/summaryTextDelta` streams readable summary text;
`summaryPartAdded` separates sections, with `summaryIndex` identifying them.
`item/reasoning/textDelta` is a separate raw-text channel when supported.
`agentMessage` text may carry `phase: commentary | final_answer`.
`item/completed` is authoritative final state.
[Official OpenAI app-server documentation](https://learn.chatgpt.com/docs/app-server#item-deltas).

The screenshot's live heading is explained directly by Codex CLI **0.153.4**:
`on_agent_reasoning_delta` accumulates reasoning summary text and extracts its
first complete Markdown `**heading**`. It updates status immediately, resets
heading extraction at section boundaries, and commits reasoning to the
detailed transcript on completion. Raw deltas enter this path only when
`show_raw_agent_reasoning` is enabled.
[Streaming implementation](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget/streaming.rs#L232),
[event routing](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget/protocol.rs#L82).

The status widget sits above the composer and separately supplies elapsed time
and the interrupt shortcut. It is client presentation, not a provider event
containing the whole rendered line.
[Status widget](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/status_indicator_widget.rs#L218).
Ferrite can preserve this behavior through normalized adapter progress and
its existing transcript/activity modules; the UI need not parse provider JSON.

## App presentation compared with Ferrite

The first supplied screenshot is the **Claude app**; the later reasoning and
working-status screenshots are **Codex CLI**. The ordinary sentences between
tool groups in the first screenshot are visible assistant commentary. Their
placement alone does not identify them as thinking blocks. Keep commentary
and provider thinking as distinct transcript content.

Claude Desktop documents three transcript modes: Normal collapses tools into
summaries while retaining full responses; Verbose exposes calls, reads, and
intermediate steps; Summary retains final responses and changes. Its chat
also makes file paths actionable. These are documented product behaviors,
not evidence of its private grouping algorithm or Markdown implementation.
[Claude Desktop transcript modes](https://code.claude.com/docs/en/desktop#switch-view-modes).
The inspected public documentation does not specify its fonts, incremental
parser, nested-list layout, or exact group/member disclosure rules. Do not
attribute terminal renderer code to the desktop app or claim those internals
were verified.

Codex CLI's published renderer is more directly inspectable:

| Concern | Verified CLI behavior | Relevant Ferrite implication |
| --- | --- | --- |
| Text hierarchy | Terminal styles distinguish bold, italic, cyan inline code, and cyan underlined links. | Responses should have their own readable style; keep code and links distinguishable from prose and secondary text. |
| Structured Markdown | Parser events retain list nesting, inline marks, tables, and hyperlinks; table layout and wrapping use available width. | Preserve original Markdown across chunks and let the existing rich renderer own structure. |
| Streaming stability | Completed newlines enter rendering; stable output and a mutable tail are separate. Tables remain mutable while arriving, because later rows can change column widths. Resize re-renders to current width. | Check unfinished lists/fences/tables and resize behavior. Do not render each incoming token as an independent Markdown document. |
| Reasoning/status | Reasoning summaries drive a compact live heading; completed reasoning is available in the detailed transcript. | A small reasoning disclosure plus separate live status prevents reasoning from overwhelming the answer. |

Sources: [Markdown styles and structure](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/markdown_render.rs#L105),
[streaming and table handling](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/streaming/controller.rs#L1),
[reasoning lifecycle](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget/streaming.rs#L232).
These observations describe the CLI, not the proprietary Codex desktop
renderer. App-server supplies semantic events, not typography or rendered UI.

### Existing Ferrite capability and concrete gaps

Read during this pass, before its presentation edits:

- [rich.rs](../../crates/ferrite/src/rich.rs) already retains a `TextViewState`
  per answer run and calls `push_str` for appended source. The
  [vendored state](../../vendor/gpui-base/src/text/state.rs) reparses the final
  block with its new suffix; the
  [Markdown parser](../../vendor/gpui-base/src/text/format/markdown.rs) supports
  nested lists, tables, links, code, and combined emphasis. Missing Markdown
  support is not a blanket explanation for the reported appearance.
- [pane.rs](../../crates/ferrite/src/pane.rs) sets monospace on the entire pane;
  responses inherit JetBrains Mono at 12px with 1.55 line height. Prose needs
  an explicit typography override to read differently from tools. Inline
  code currently has a raised background and gray text; links have white
  text and native underlining. Adjust those deliberately when prose changes.
- Thinking currently renders as a full muted paragraph. Compact disclosure
  and an independently updating status line address different needs: reading
  details versus knowing work continues.
- Tool grouping and each member's detail expansion should have independent
  identities. Opening a group should reveal inspectable call summaries;
  opening one call should reveal its input/output. This is a Ferrite design
  recommendation, not a claim about Claude Desktop's undocumented internals.

No parser replacement or additional provider-specific UI transport is needed
for these presentation changes. Any deeper Markdown fidelity fix should begin
with a concrete unsupported or misrendered example.

## Codex asynchronous questions

Verified against installed `codex-cli 0.153.4` and the matching published
source/schema. Structured async questions shipped in 0.153.0 and depend on
model-catalog support.
[Official release notes](https://learn.chatgpt.com/docs/changelog#codex-cli-01530).

**Two distinct protocols must remain distinct.** The new
`request_user_input_async` tool emits `item/started` and `item/completed`
notifications containing this item:

```json
{
  "type": "agentMessage",
  "id": "tool-call-id",
  "text": "Which scope?\n- Small\n- Full",
  "phase": "final_answer",
  "delivery": "async",
  "questions": [{"title": "Which scope?", "options": ["Small", "Full"]}]
}
```

App-server preserves both structured fields; neither has an experimental
client-capability gate. Question `options` may be absent/null for free text.
The handler completes immediately with `{"accepted":true}` and expects the
answer later as a new user message. Here `phase: final_answer` does **not**
mean the turn ended. The first suggested option may be preselected but is
never submitted automatically.
[Handler](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/core/src/tools/handlers/request_user_input_async.rs),
[app-server schema and conversion](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L249).

Availability requires a root agent whose model metadata
`experimental_supported_tools` includes `request_user_input_async` or its
legacy catalog name `send_user_message_async`. No extra Ferrite configuration
switch enables an unsupported model.
[Registration gate](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/core/src/tools/spec_plan.rs#L1161).

While a turn runs, submit a self-contained question/answer as ordinary user
text using `turn/steer` with `threadId`, `input`, and `expectedTurnId`. The
server rejects a missing/mismatched active turn; it does not emit another
`turn/started`. When idle, use `turn/start`. There is no dedicated
async-question answer RPC.
[Active-turn input](https://learn.chatgpt.com/docs/app-server#steer-an-active-turn).

By contrast, `item/tool/requestUserInput` is a server request with
`threadId`, `turnId`, `itemId`, question IDs, and `isBlocking` (defaults true
when omitted). Answer the **original JSON-RPC request ID** with
`{"answers":{"question_id":{"answers":["selected label or free text"]}}}`.
`autoResolutionMs` is deprecated in the pinned schema; do not use it to infer
blocking semantics. This request supports richer option descriptions and
secret-input metadata, unlike the async shape.
[Request/response schema](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L1729).

**Ferrite implication:** its existing `questions.rs` implements Claude's
blocking `AskUserQuestion` approval protocol, including answers keyed by full
question text; Codex async questions cannot use that serialization. Reuse
question controls through a normalized model, but keep async questions out of
blocking Decisions, preserve ongoing progress, identify them by thread/item,
and avoid displaying their fallback text a second time. Accept free text even
without choices. Keep the answer draft until submission succeeds. At research
time `CodexSession::send` always called `turn/start`, so active-turn steering
needs explicit adapter support. These are implementation recommendations,
not claims of completed behavior.

## Claude questions during ongoing work: clarification

Claude does have **structured prompts during a live session**; describing all
its questions as ordinary chat messages would be incorrect. `AskUserQuestion`
uses `canUseTool` with its question/options payload. Returning
`behavior: "allow"` plus `updatedInput.answers` resolves the request. The
callback awaits a response without ending the enclosing `query()` call.
[Official question handling](https://code.claude.com/docs/en/agent-sdk/user-input#handle-clarifying-questions).

This does not imply that every other task stops. Claude documents concurrent
background subagents whose permission prompts appear in the main session;
the asking subagent can resume after its prompt is answered. Forked agents
retain their parent's tool definitions, whereas ordinary background subagents
have a smaller tool set. A prompt and continuing activity can therefore
coexist; appearance alone does not identify which execution is awaiting input.
[Background agents and prompts](https://code.claude.com/docs/en/sub-agents#run-subagents-in-foreground-or-background).

Latest published Agent SDK **0.3.261**, inspected from its official npm package,
also exposes two distinct host interactions:

- `onUserDialog`: `dialogKind`, opaque `payload`, optional `toolUseID`, and
  request ID/cancellation signal. Its declaration explicitly describes
  **blocking** dialogs. Hosts declare supported kinds through
  `supportedDialogKinds`; this is not a general async question channel.
- `onElicitation`: MCP-originated form or URL prompts, including optional JSON
  schema. Response uses accept/decline/cancel, separate from `AskUserQuestion`.

[Published 0.3.261 declarations](https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.3.261/sdk.d.ts).

The installed Claude **2.1.261** embedded implementation marks
`AskUserQuestion` concurrency-safe but also `requiresUserInteraction`, and its
permission check returns `behavior: "ask"`. Additional choice/text/number
question variants exist behind a host launch option; that option is absent
from the inspected public `ToolConfig` declaration. Neither fact establishes
a universally available immediate-return async question API. No such Claude
equivalent to Codex's `request_user_input_async` was verified in this research;
this is a bounded finding, not a claim about every proprietary app flow.
[Inspected CLI version](https://registry.npmjs.org/@anthropic-ai/claude-code-darwin-arm64/2.1.261).

## GPUI question controls inspected for implementation

Inspected the installed, pinned `gpui-component 0.6.0` sources before changing
forms: `src/radio.rs`, `src/checkbox.rs`, `src/input/input.rs`,
`src/button/button.rs`, and `src/group_box.rs` in the Cargo registry.
The `gpui-kit` facade re-exports these through `gpui::component`.
`RadioGroup::vertical` owns single-choice grouping; `Checkbox` supports multiple
choices. Both accept custom content and accessible labels, so option descriptions
can wrap without inventing painted choice marks. `Input` retains native editing
state; `Button` handles activation/disabled state. `GroupBox::content_style`
provides the same surface seam already used by Ferrite attachments.

Implementation keeps provider JSON in the Codex adapter. A typed delivery field
in the shared request vocabulary separates blocking questions from async input;
the question UI never checks a provider or parses app-server packets. Existing
Main/child question forms are consolidated; the obsolete digit-key form is removed.

Batch presentation now counts observed tool kinds (search patterns, file reads,
file changes, shell commands, web searches, other tools), switches to past tense
on completion, and exposes the currently running input in a short wrapping
preview. Member rows prefer a provider-supplied title/description/reason, retaining
the actual command/input and output for the second disclosure.

Focused validation in this pass: the async delivery regression covers structured
message deduplication, continued execution, a rejected steer, retained questions
across turn completion, and an acknowledged idle reply. Existing UI checks cover
native choices and typed answers for Main and unattributed requests. A discovered
focus bug was fixed: a single Main question now participates in the same native
control focus preservation as child and multi-request forms.
