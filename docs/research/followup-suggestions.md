# Follow-up suggestions from providers

Researched 2026-09-06 against the two CLIs Ferrite ships with: Codex 0.153.4
and Claude Code 2.1.261. Protocol schemas, shipped binaries, and probe runs
against the local CLI.

Question: can Ferrite show, in the Composer's idle line, a follow-up prompt
predicted from the last response — and can Tab accept it?

## Codex has nothing

`codex app-server generate-json-schema -o <dir>` emits the whole app-server
protocol. Nothing in it carries a suggested next user prompt: `turn/completed`,
`item/completed` and the thread notifications have no such field, and neither
`codex --help` nor `codex exec --help` offers a flag. The only near-miss
identifiers are unrelated — `CollabAgentTool` has a `followupTask` variant,
which is *a tool one agent calls to task another*, and
`installSuggestionPluginNames`, which is about plugins.

## Claude Code has it, but it could not be made to fire

`claude --help` documents the flag:

> `--prompt-suggestions` — Enable prompt suggestions. In print/SDK mode, emits
> a `prompt_suggestion` message after each turn with a predicted next user
> prompt

The SDK schema in the binary confirms the frame:
`{type: "prompt_suggestion", suggestion, uuid, session_id}`, described as
*"Predicted next user prompt, emitted after each turn when promptSuggestions is
enabled."*

**It did not emit in any local configuration.** Probed: `-p` one-shot;
`--input-format stream-json` with stdin held open 35s (the shape Ferrite runs);
`--prompt-suggestions true`; and `CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION` set to
both `1` and `true`. No frame in any of them.

Reading the emit site in the binary, the enable-check is:

```
env CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION === false -> off
env ... === true                                   -> on
!gate("tengu_chomp_inflection")                    -> off   (remote flag)
non-interactive                                    -> off
settings.promptSuggestionEnabled !== false
```

and the SDK emit is further guarded by `promptSuggestions && shouldQuery !==
false`. Past that, a candidate must survive a filter that suppresses: under 2
words (unless the word answers on its own), over 12 words, 100 chars or more,
multiple sentences, any markdown, "evaluative" replies (*thanks*, *looks
good*), and anything in Claude's own voice (*let me*, *I'll*, *here's*).

So the feature is real and the shape is right, but a cockpit cannot depend on
one arriving — it is gated on a server-side flag and then filtered hard.

> Correction: an earlier note in this repo said neither provider supported
> this. That was wrong for Claude Code. The first sweep searched for
> `suggestedPrompt` / `promptSuggestion` and dismissed the real hits as
> minified noise; the identifier is `prompt_suggestion` and the flag is
> `--prompt-suggestions`.

## What Ferrite does

`ferrite-core::suggest` asks for its own prediction, the same way for both
providers so two Panes side by side behave alike. One `claude` run per finished
Main turn:

```
claude -p --model haiku --output-format json
       --no-session-persistence --safe-mode
       --tools "" --permission-prompts none
       --system-prompt <Ferrite's>
```

with the last exchange on stdin. `--tools ""` is the cost lever — the tool
schema dwarfs the prompt — and `--safe-mode` keeps the run from reading the
operator's CLAUDE.md, settings, skills, hooks or MCP servers.
`--no-session-persistence` keeps it out of their `/resume` picker. It reuses
the CLI's own credentials; Ferrite holds no API key.

**Measured**: ~354 input tokens, ~$0.005, ~9s wall (mostly CLI startup). One
call per finished turn per Thread, on by default. A grid of N working Threads
costs N calls per round of turns.

The reply goes through the same filters Claude Code applies to its own
suggestions — the failure modes are the same because the job is the same, and a
suggestion the vendor would suppress is not one to show. A refused reply leaves
the generic idle line.

`ferrite-core::followup` then decides whether the prediction is the right thing
to show: a pending Decision outranks it, so does a closed Session, and a
streaming Thread shows none at all (the response it was predicted from has been
superseded). The renderer owns every sentence.

Tab on an empty Composer accepts the prediction into the line as editable text
and does **not** send it — a guess is not consent to start a turn. The accept
condition is `followup::suggest` itself, the same call the idle line renders
from, so the key and the ghost text cannot disagree.
