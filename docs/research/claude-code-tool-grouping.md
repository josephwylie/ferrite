# Claude Code CLI tool grouping

Researched 2026-09-05 for [Ferrite #22](https://github.com/josephwylie/ferrite/issues/22).
Scope: the terminal CLI's compact activity transcript, especially the four
separate shell calls in the operator's screenshot. Research and proposed
behavior only; this note does not establish that Ferrite implements it.

## Result

Claude's useful pattern is **one activity summary that updates in place, with
details available on demand**. Its grouping is more sophisticated than placing
four full tool rows inside a border. Consecutive shell calls are a supported
grouping case; semantic read/search groups also exist. The evidence below
supports adopting that interaction without pretending we have Claude's exact
internal algorithm.

## Evidence and limits

- Local read-only `claude --version` returned **2.1.261**. No model query was
  run, no session was resumed, and no live CLI interaction was recorded.
- Official documentation was read on the research date. It describes both
  classic and fullscreen renderers; click interactions below specifically
  concern fullscreen. Documentation is not a version-pinned UI specification.
- Release history was inspected at
  [commit d7dbd9a](https://github.com/anthropics/claude-code/blob/d7dbd9a09f59775726ed14bbea8fc9dfdff62f7b/CHANGELOG.md),
  with release-specific links below. The inspected public repository has docs,
  examples, plugins, and release history; I did not locate its CLI transcript
  renderer implementation there. Unofficial source mirrors were not used.
- Exact minimum group size, full command classifier, all group boundaries,
  error auto-expansion, animation timing, and disclosure persistence remain
  **unverified**. A release note proves the stated behavior or correction, not
  every detail of today's implementation.

## Verified CLI behavior

| Behavior | First-party evidence |
| --- | --- |
| Consecutive shell calls share a summary; internal todo/task updates should not fragment it. | [2.1.234](https://github.com/anthropics/claude-code/releases/tag/v2.1.234) fixed that fragmentation. |
| Some Bash calls participate in read/search grouping, rather than being classified solely by tool name. | [2.1.0](https://github.com/anthropics/claude-code/blob/d7dbd9a09f59775726ed14bbea8fc9dfdff62f7b/CHANGELOG.md#210) corrected counting for commands such as `cat` and `ls`. |
| Active summaries use present tense; completed summaries use past tense. The active file or search pattern can appear underneath. | [2.1.20](https://github.com/anthropics/claude-code/blob/d7dbd9a09f59775726ed14bbea8fc9dfdff62f7b/CHANGELOG.md#2120), [2.1.45](https://github.com/anthropics/claude-code/blob/d7dbd9a09f59775726ed14bbea8fc9dfdff62f7b/CHANGELOG.md#2145). |
| Directory listings have their own description; MCP queries can summarize by server. | [2.1.89](https://github.com/anthropics/claude-code/blob/d7dbd9a09f59775726ed14bbea8fc9dfdff62f7b/CHANGELOG.md#2189), [2.1.81](https://github.com/anthropics/claude-code/blob/d7dbd9a09f59775726ed14bbea8fc9dfdff62f7b/CHANGELOG.md#2181). |
| Running summaries show elapsed time, with time visually subordinate to counts. | [2.1.210](https://github.com/anthropics/claude-code/releases/tag/v2.1.210), [2.1.234](https://github.com/anthropics/claude-code/releases/tag/v2.1.234). |
| Interrupted execution must not look successfully finished; truncated failures remain expandable. | Relevant fixes in [2.1.246](https://github.com/anthropics/claude-code/releases/tag/v2.1.246) and [2.1.136](https://github.com/anthropics/claude-code/blob/d7dbd9a09f59775726ed14bbea8fc9dfdff62f7b/CHANGELOG.md#21136). |

In fullscreen, clicking a collapsed result expands both its call and result;
another click collapses them. Only entries with additional content are
clickable. Scrolling upward pauses following; returning to the bottom resumes
it. This matters when a group changes height while the operator reads older
output. [Fullscreen interaction and scrolling](https://code.claude.com/docs/en/fullscreen).

There is also a global escape hatch: `Ctrl+O` opens the detailed transcript,
and `Ctrl+E` toggles all content there. MCP calls normally summarize by server
and count. [Interactive mode](https://code.claude.com/docs/en/interactive-mode#transcript-viewer).
Fullscreen `/focus` is a separate, quieter view retaining the latest prompt,
an activity summary with edit statistics, and the final response.
[Focus view](https://code.claude.com/docs/en/fullscreen#search-and-review-the-conversation).

## Display grouping is not execution batching

The API can emit several `tool_use` blocks in one assistant response, but its
caller decides whether to execute them concurrently or sequentially. Each
result still links to its individual call via `tool_use_id`.
[Parallel tool execution semantics](https://platform.claude.com/docs/en/agents-and-tools/tool-use/parallel-tool-use#execution-semantics).

Consequently, a visual group is not evidence that its members ran in parallel.
That conclusion is an inference from the API contract, not a claim about
Claude's private scheduler. Ferrite should only display a concurrency claim
when its provider supplies reliable execution evidence. An assistant sentence
saying that grouping worked is also not evidence of rendered grouping.

Partial events describe message/content construction. In particular, the
documented flow places tool execution after the completed assistant message.
Do not confuse the end of streamed tool arguments with successful execution.
[Streaming message flow](https://code.claude.com/docs/en/agent-sdk/streaming-output#message-flow).

## Proposed Ferrite behavior

These are implementation recommendations, **not verified Claude rules**.

For the screenshot, the normal completed view should be approximately:

```text
▸ Ran 4 shell commands                 ✓

Done—ran pwd, date, uname, and git status ...
```

Opening the summary reveals the four commands in their original order, with
each command's own output and outcome. Successful output and four separate
`exit 0` badges should not consume transcript height while the group is closed.
Keep Ferrite's existing typography and spacing; a heavy card is unnecessary.

| Situation | Proposed display |
| --- | --- |
| Calls still executing | One summary with completed/running counts, plus one current-command preview. Add elapsed time only from reliable timing data. |
| All succeed | One closed summary with total calls. |
| Any fail | Persistent failure count and concise error preview, even when closed; initially reveal the failed member. Successful siblings stay compact. |
| Approval needed | Keep the existing actionable approval visible. Never bury its choices inside a closed activity group. |
| Interrupted or unknown | Preserve an explicit interrupted/unknown outcome when supplied; never infer success because streaming stopped. |
| Operator expands it | Preserve that choice as calls arrive or settle. Do not repeatedly auto-collapse a group being inspected. |

Suggested grouping rules:

1. First support consecutive `Bash`/`commandExecution` calls, with at least two
   calls forming a group. Count tool invocations; a shell script with four
   internal commands is still one invocation.
2. Preserve chronology. Split at visible assistant prose, user prompts, edits,
   approvals, notices, and session/turn boundaries. Skipping known internal
   bookkeeping can be a deliberate later extension; do not skip arbitrary text.
3. Keep a stable group identity based on the first member, and retain individual
   call IDs and original output. Grouping belongs in presentation, not replay
   or provider execution.
4. Add semantic read/search summaries through a conservative classifier:
   structured `Read`, `Grep`, and `Glob` first. Classify Bash read/list commands
   only when their structure is understood. An arbitrary command containing
   `cat`, `rg`, or `ls` is not sufficient evidence.
5. Provide a detailed-transcript action for inspection. Keep text selection,
   copying, links, and scrolling usable inside expanded output; selecting text
   must not accidentally toggle the group.
6. Respect the scroll anchor when groups grow or collapse. At the tail, keep
   following; while the operator reads history, preserve their position.

The local [transcript model](../../crates/ferrite-core/src/transcript.rs) already
retains call identity, summary, bounded output, and `Running`/`Ok`/`Failed`
states. It currently has no explicit batch ID or interrupted state.
The [pane renderer](../../crates/ferrite/src/pane.rs) has per-tool disclosure
and activity wording. Those are useful existing seams; no replacement agent
loop is necessary. These observations concern the inspected working tree and
may change during the ongoing migration.

## Minimal validation

Use the reported four-call fixture: it must produce one closed summary and
expand to four correctly paired command/results. Reuse it with one failure and
one still-running call to check visible outcomes and stable disclosure. A
native interaction pass should cover expand, selection/copy, and streaming
while scrolled upward. This is enough to verify the immediate grouping change;
semantic classification can be validated when introduced.
