# Subagent wire fixtures

Reduced fixtures from authenticated CLI captures on macOS, 2026-09-05, using
fresh temporary workspaces and bounded synthetic tasks. Claude Code **2.1.261**
used stream-json with `--forward-subagent-text`; Codex **0.153.4** used one
app-server JSON-RPC connection. No provider runs during fixture replay.

| Fixture | Model | Retained case | Frames |
| --- | --- | --- | ---: |
| `claude-overlap-2.1.261.jsonl` | `claude-sonnet-5` | Two background children, shared message IDs across distinct block UUIDs, Bash task exclusion, separate usage, background Main results | 47 |
| `claude-nested-2.1.261.jsonl` | `claude-sonnet-5` | Direct parent and grandchild, forwarded prompts/text/tools, task completion | 27 |
| `claude-reuse-2.1.261.jsonl` | `claude-sonnet-5` | Persisted child reuse via SendMessage; stable original spawn ID with new invocation alias; autonomous result origin | 28 |
| `claude-decisions-2.1.261.jsonl` | `claude-opus-5` | Two overlapping child Write approvals, original request/agent/tool IDs, allowed and denied results | 85 |
| `codex-overlap-reuse-0.153.4.jsonl` | `gpt-6-astra` | Concurrent children, three child turns, same-thread reuse, scoped deltas/usage/tools, names from history | 116 |
| `codex-nested-0.153.4.jsonl` | `gpt-6-astra` | Immediate nested parentage, grandchild final text, summary turns versus full history | 90 |

These are **reduced captures**, not complete byte-identical transcripts.
Original IDs, relevant event ordering, settled content and text deltas remain.
Startup/rate-limit/deprecation chatter, redundant streamed tool-input fragments,
timestamps, signatures and unused metadata were removed. JSONL keeps ordinary
spacing and one provider envelope per line; `.gitattributes` preserves its bytes.
Workspace/account identifiers were sanitized before reduction. Claude overlap
and nested sessions were ephemeral; reuse used normal provider persistence.

Full captures, launch arguments and original manifests remain in
[commit 62485e9](https://github.com/josephwylie/ferrite/tree/62485e9288d909bbb181da39f90227bd3a2e4c55/spikes/subagents):
Claude `claude/{overlap,nested,reuse-persisted}.jsonl`, decisions
`decisions/claude.provider.jsonl`, and Codex `codex/{overlap-reuse,nested}.jsonl`.
Only these six captures are required by the core decoder and Session tests.
