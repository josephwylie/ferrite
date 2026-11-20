# Decisions

When an agent wants permission (a tool call, a plan, a question), its Thread turns
`blocked`, the Pane draws the request as a card ("Allow Claude to run this command?"
with `Deny` / `Allow once`), the sidebar row gets an amber dot, and the user answers.
The agent continues with that answer.

## Sub-features

- `decision-raise` a tool call under permission mode `default` blocks the Thread and shows the card.
- `decision-list` lists the pending Decision with its tool, input and suggestions.
- `decision-allow` allowing runs the tool and the turn finishes.
- `decision-deny` denying stops the tool; the agent reports it was refused.

## How to get to it (user POV)

- `y` / `n` / `a` from any Pane, `cmd-enter` on the card, or the `Deny` / `Allow once` buttons.
- `cmd-d` jumps to the next Decision; `cmd-i` lists Threads that need you.
- Scripting: `ferrite decision list`, `ferrite decision answer`.

## Driving it with fv

Preconditions:

- Baseline run (scratch settings use permission mode `default`, so Bash asks).
- Thread `1` started as in [threads.md](./threads.md) `thread-start`.

- **Raise.** Run `fv cli -- agent prompt 1 "Use the Bash tool to run: touch made.txt . Do nothing else." --wait --timeout-ms 90000`. Exit 0 with `attention: blocked`. `fv shot decision-pending`: card `Allow Claude to run this command?` showing `$ touch made.txt`, `Needs your input` badge, amber dot on the sidebar row. `ls /tmp/fv-$FV_RUN/repo/made.txt` fails.
- **List.** Run `fv cli -- decision list`. One entry: `thread: 1`, `tool_name: "Bash"`, `input.command: "touch made.txt"`; note `decision.id`.
- **Allow.** Run `fv cli -- decision answer 1 <decision.id> --answer '{"kind":"allow","input":<the listed input object>}'`, then `fv cli -- agent wait 1 --timeout-ms 60000`. Answer `attention: done`. `ls /tmp/fv-$FV_RUN/repo/made.txt` succeeds; `fv cli -- agent read 1` shows `● Bash (touch made.txt)`. `fv shot decision-answered`: the card is gone.
- **Allow from the keyboard.** Prompt from the composer: `fv click --selector composer-block`, `fv type "Use the Bash tool to run: touch kb.txt . Do nothing else."`, `fv key enter`. `fv cli -- agent wait 1 --until blocked --timeout-ms 90000` answers `blocked`; `fv find approval-prompt` answers `found`; `fv shot decision-kb-pending` shows the card. Then `fv key cmd-enter` (the card's `Allow once ⌘↩`); `fv cli -- agent wait 1 --timeout-ms 60000` settles `done`, `kb.txt` exists, `fv shot decision-kb-answered` shows the card gone and `Created kb.txt`. `fv key y` / `n` / `a` are the other entry points: allow, deny, always allow.
- **Deny.** Repeat Raise with `remove.txt`, then answer `{"kind":"deny","message":"Not now."}`. The file is not created and the transcript shows the refusal.

## Gotchas

- `agent prompt` refuses with `agent_blocked` while a Decision is pending; answer it first.
- The `allow` answer must carry the tool's `input` object; copy it from `decision list`.
- If the scratch settings were edited to `bypassPermissions`/`acceptEdits`, no Decision is raised and this journey fails. That is the precondition, not the feature.
- `ferrite --schema` lists every answer kind (`choose`, `questions`, `form`, `cancel`) for the other Decision kinds.
