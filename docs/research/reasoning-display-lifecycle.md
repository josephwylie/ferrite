# Reasoning display lifecycle

Researched 2026-09-06 by Codex. Installed Claude Code `2.1.263` and Codex source
pinned to `rust-v0.153.4`; Ferrite
comparison pinned to draft PR #53, `d384760`. Read-only source investigation;
no model requests, product edits, or tests run. Source behavior below is distinct
from recommendations for Ferrite.

## Codex: what appears when

| Event/content | Live status | History |
| --- | --- | --- |
| Turn starts | `Working` (unless startup status owns the header). | No reasoning row. |
| Reasoning item starts | No special handling in the CLI item-start router. | No reasoning row. |
| Summary text arrives before a complete `**bold element**` | Existing status stays. No first-line fallback. | Deltas only accumulate in memory. |
| First complete, nonempty bold element arrives | Its trimmed text becomes the thinking header. It need not start the first line. Safety buffering and unified-exec waiting take precedence. | Still no reasoning history cell. |
| Summary part boundary | Reset bold extraction; keep previous status until a new heading arrives. | Save the preceding part; preserve part boundaries. |
| Reasoning item completes | Clear reasoning buffers/extraction state; this handler does not itself reset the displayed status. | Commit one cell from accumulated parts, using the rules below. |
| Turn completes | Stop the running-task status. | Completed history remains. |

Sources: [turn start/completion](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget/turn_runtime.rs#L70),
[item-start routing](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget/protocol.rs#L352),
[streaming and section boundaries](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget/streaming.rs#L232),
[bold extraction](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget.rs#L2001).

The comment describing completion as “transcript-only” is insufficient evidence:
the called factory conditionally makes content visible in normal history.
The actual completed-cell behavior is:

| Supplied summary parts | Normal history | Detailed transcript |
| --- | --- | --- |
| `**Heading**\n\nBody` | Body, dim italic Markdown; leading heading removed. | Same body. |
| `**Heading**` (optional trailing whitespace) | Heading, rendered as Markdown. | Same heading. |
| Plain prose, or incomplete bold heading | Hidden. | Full supplied text as Markdown. |
| A part whose entire body is `<!-- -->`, optionally preceded by a bold heading | No output for that part; no blank row. Its heading could already have appeared in live status. | No output for that part. |
| Literal comment within real prose/code, e.g. ``Use `<!-- -->` here`` | Preserved when the containing content qualifies for normal history. | Preserved. |

Parts are trimmed, filtered separately, and joined with blank lines. Only the
leading heading followed by a newline is stripped from the combined body;
later headings can remain in the body. There is no per-part expandable control
in this CLI cell. [Factory/splitting implementation](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/history_cell/messages.rs#L623),
[display/transcript rendering](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/history_cell/messages.rs#L297),
[asserted rendering cases](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/history_cell/tests.rs#L2588).

## Codex: protocol, completion, and configuration

The app-server contract distinguishes readable `summary` from optional raw
`content`. `summaryTextDelta` carries `itemId` and `summaryIndex`;
`summaryPartAdded` marks the section boundary. Raw `textDelta` is a separate
channel indexed by `contentIndex`. `item/completed` contains the authoritative
final item. These are content/lifecycle contracts; the inspected documentation
does not require the CLI's bold-heading convention, timeline timing, or a
specific disclosure control. [Official OpenAI app-server documentation](https://learn.chatgpt.com/docs/app-server#events),
[pinned notification types](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L1443).

**CLI limitation, not a contract to copy:** during live completion, the CLI
flushes buffered deltas; it only reads the completed item's `summary`/`content`
when replaying history. Thus a completed-only summary is used on replay, but
not reconstructed by this live handler. Ferrite should retain authoritative
snapshot reconciliation. [Live/replay branch](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget/replay.rs#L131).

Summary selection is **model-specific**, not globally `auto`: selected settings
override the pinned model's `default_reasoning_summary`. The bundled 0.153.4
catalog specifies `none` for Astra, Sol, Terra, Luna, GPT-5.5, GPT-5.4 and
GPT-5.4-mini; GPT-5.2 specifies `auto`. Runtime catalogs/configuration can differ.
`auto`, `concise`, `detailed`, and `none` are supported settings; `none` or a
model without summary-parameter support omits the request parameter.
[Settings resolution](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/core/src/session/step_settings.rs#L53),
[bundled catalog](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/models-manager/models.json#L36),
[setting enum](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/protocol/src/config_types.rs#L57),
[request construction](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/core/src/client.rs#L906).

`show_raw_agent_reasoning` defaults to false; enabling it routes supported raw
deltas into the same CLI rendering path and includes raw completed content on
replay. It does not reconstruct unavailable reasoning.
[Configuration default](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/core/src/config/mod.rs#L4250),
[raw routing](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/tui/src/chatwidget/protocol.rs#L82).

## Ferrite comparison at d384760

Confirmed from the adapter, fold, and rendering code:

- **Live timing differs:** each nonempty summary delta creates/updates a
  persistent `Body::Thinking` immediately; the view renders it immediately.
  The native item-completion boundary is not retained as a reasoning block
  completion state. [Transcript fold](../../crates/ferrite-core/src/transcript.rs#L707),
  [view](../../crates/ferrite/src/pane.rs#L3631).
- **Live heading fallback differs:** `progress::headline` falls back to the
  first line when no complete bold element exists. A chunk `**Checking fold`
  therefore appears early as `Checking fold`; Codex keeps the previous status
  until the closing `**`. [Heading extraction](../../crates/ferrite-core/src/progress.rs#L329).
- **Placeholder gap:** `**Heading**\n\n<!-- -->` becomes a heading plus a
  disclosure, because the parser considers every nonempty remainder to be
  details. Codex removes this standalone empty part at completion. Preserve
  literal comments inside real text. [Disclosure split](../../crates/ferrite/src/pane.rs#L3502).
- **Presentation policy differs:** Ferrite keeps a heading visible and
  collapses the body; Codex shows completed headed bodies directly and removes
  their leading heading. Ferrite also displays short unheaded prose directly,
  where the CLI limits it to detailed transcript. These can be deliberate
  Ferrite choices, but are not established CLI parity. [Rendering](../../crates/ferrite/src/pane.rs#L3629).
- **Good existing reconciliation:** Ferrite identifies parts by `(item_id,
  summary_index)`, replaces each completed snapshot in place, accepts
  completed-only summaries, and prevents late snapshots restarting a finished
  turn. Preserve these properties. [Parser](../../crates/ferrite-core/src/providers/codex/wire.rs#L204),
  [fold](../../crates/ferrite-core/src/transcript.rs#L707),
  [late-completion regression](../../crates/ferrite-core/tests/progress_lifecycle.rs#L279).
- **Whole-item snapshot gap:** completion emits only parts present in the
  final array. If two parts streamed but the authoritative array contains one
  or zero, omitted old parts have no removal event and remain in the fold.
  This is an inspection-derived edge case; no captured provider case or
  executable regression was produced during this research. [Main parsing](../../crates/ferrite-core/src/providers/codex/wire.rs#L204),
  [child completion](../../crates/ferrite-core/src/providers/codex/activity.rs#L866).
- **Intentional opt-in:** Ferrite sends `summary: "detailed"`, including
  idle question replies. That overrides user/model summary selection; the
  app-server setting persists to later turns. It is a valid display policy,
  not a CLI default or a guarantee of nonempty details. Ferrite ignores raw
  reasoning, consistent with the CLI's default display policy.
  [Turn submission](../../crates/ferrite-core/src/providers/codex.rs#L452),
  [question reply](../../crates/ferrite-core/src/providers/codex/questions.rs#L97),
  [persistent override contract](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/app-server-protocol/src/protocol/v2/turn.rs#L233).

### Suggested verification seams

Use `provider_progress.rs` to replay a split bold heading, section boundary,
two parts, complete snapshot, tool start, and turn completion through the real
adapter. Assert status and visible-history timing at each step; include empty
and shrinking final arrays, repeated completions, and main/child attribution.
Existing tests cover part correction and deduplication, but not those timing
and shrink cases. [Existing replay regressions](../../crates/ferrite-core/tests/provider_progress.rs#L248).

Use the existing GPUI tests to assert that placeholder-only content has no
empty disclosure, real inline comments survive, and any chosen delayed-history
policy does not duplicate live status or erase expanded/selected content.
The current UI regression explicitly renders a body during streaming, so a
timing change requires updating that product expectation.
[Existing disclosure tests](../../crates/ferrite/src/cockpit.rs#L8676).

## Claude: content and streaming contract

Readable reasoning arrives in `thinking` blocks when `display: summarized`
is requested. Thinking can precede the answer and recur between tools.
`thinking_delta` text accumulates until `content_block_stop`; signatures are
not display text. Delivery can pause or arrive in chunks. Empty blocks need
no transcript row. Enabling display does not force the model to think.

Some models also produce user-facing progress updates immediately before
tools. These are separate `thinking` blocks, despite being commentary rather
than reasoning. With `display: updates` (beta), only progress blocks carry
readable text; with `summarized`, both categories can. Raw Messages API
thinking text under `summarized` does not identify the category. Therefore,
first-line wording and block length are not sound classifiers.
[Thinking and streaming contract](https://platform.claude.com/docs/en/build-with-claude/thinking#streaming-thinking),
[progress-update semantics](https://platform.claude.com/docs/en/build-with-claude/thinking#progress-updates-between-tool-calls).

The Agent SDK defaults to completed assistant messages. Opting into partial
messages adds stream events alongside completed messages; appending both
duplicates content. Main-agent deltas can be shown as received. Subagent
token-level events are not forwarded, so child text becomes available through
completed messages. Block completion is separate from tool execution and from
the SDK result that ends the turn.
[SDK message flow](https://code.claude.com/docs/en/agent-sdk/streaming-output#message-flow).

## Claude: installed CLI evidence

Read-only inspection of the installed official executable at
`~/.local/share/claude/versions/2.1.263`, SHA-256
`ef5d2909c8af49f31ab6d5487e90316777bc2fac170adfe8160716caa8aaf4f9`.
These observations describe terminal renderer branches and embedded SDK
schema, not a fresh live model session or Claude Desktop's private renderer.
The native Bun executable retains readable JavaScript modules; offsets below
identify the inspected evidence without checking vendor code into Ferrite.

- The CLI accepts `--thinking-display summarized|omitted`. The SDK maps its
  display option to this flag. `showThinkingSummaries` opts into summaries;
  the installed settings lookup defaults to false (offset 158460603).
- Default request selection can choose connector/progress text with the
  `updates` beta, subject to model, provider, and rollout gates. The fact that
  `updates` is absent from the CLI flag choices does not establish that the
  CLI never requests it (selection at 165680324). Ferrite explicitly requests
  `summarized`, a valid opt-in rather than a replica of stock defaults.
- The generic content renderer routes nonempty narration-tagged thinking to
  `AssistantNarrationSummaryMessage` before its transcript/verbose gate.
  That renderer presents ordinary Markdown in the conversation. Ordinary
  thinking takes the transcript/verbose branch and renders muted Markdown;
  redacted thinking takes a generic thinking indicator branch (180372333;
  narration renderer export at 187631472). The live activity indicator is
  separate from this stored content renderer. These branches do not establish
  every higher-level UI mode's visibility policy.
- The embedded assistant-envelope schema exposes optional
  `narration_block_indexes`: zero-based indexes into **that frame's**
  `message.content`. Its description explicitly supplies this classification
  for renderers, avoiding signature decoding. Listed blocks with empty text
  still receive ordinary empty-thinking treatment. Unlisted blocks and older
  emitters default to ordinary thinking (schema at 158540033; encoder at
  164288942).
- This metadata is marked internal. It is absent from the previously examined
  public Agent SDK `0.2.118` declaration. Support it as optional observed
  metadata; do not require it or reverse-engineer encrypted signatures in
  Ferrite. The installed classifier and encoder establish that it exists in
  this CLI version, not that every supported version emits it.
  [Previously published SDK declaration](https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.2.118/sdk.d.ts).

## Claude: Ferrite comparison at d384760

- Ferrite opts into partial messages and summarized thinking. It displays
  readable main-agent thinking as deltas arrive, preserves block boundaries,
  and avoids empty reasoning rows. These are supported SDK consumption
  choices. [Launch options](../../crates/ferrite-core/src/providers/claude.rs),
  [stream decoding](../../crates/ferrite-core/src/providers/claude/wire.rs),
  [thinking fold](../../crates/ferrite-core/src/transcript.rs#L860).
- Ferrite ignores `narration_block_indexes`. All readable thinking uses the
  same muted reasoning/disclosure presentation, including progress narration
  on models that supply it. This loses a distinction the current CLI exposes
  specifically for rendering. Main-agent completed assistant thinking is
  ignored to avoid duplicating deltas; child snapshots map thinking into
  `ExecutionEvent::Thinking` without the category. [Snapshot decoder](../../crates/ferrite-core/src/providers/claude/activity.rs#L374).
- Correct classification needs message/block reconciliation, including
  metadata that arrives only at completion. It must update the existing
  streamed block rather than append the same text again. Use stream
  `message_start.message.id` plus content index and match the completed
  message, preserving main/child attribution. Do not infer progress narration
  from prose or promote signature contents into the transcript.

## Audit conclusion

PR #53 fixes the reported repeated expansion and distant chevron. The compact
preview is derived locally from the received provider text. Neither provider
promises a separate short summary plus a second, fuller explanation; requesting
Codex `detailed` cannot guarantee additional text for every item.

**The PR is not proof of complete provider display parity.** Its disclosure
layout and explicit summary opt-ins are Ferrite policy. Codex timeline/status
timing differs from its CLI; Claude narration loses its distinct category.
The SDK/app-server contracts permit custom host layouts and streaming choices,
so timing/layout differences alone are not protocol violations. Placeholder
disclosures and incomplete whole-item snapshot reconciliation are additional
concrete follow-up gaps identified above. No product fixes for these newly
identified gaps were made during this audit.

For provider-faithful behavior, keep the requested compact Ferrite disclosure,
retain actual provider text and boundaries, represent live progress separately
from stored detail, and honor supplied narration/completion metadata. Verify
those transitions with provider event replays before claiming parity. An exact
copy of either terminal UI would require additional explicit presentation
choices; it would also remove Ferrite's requested expandable preview.
