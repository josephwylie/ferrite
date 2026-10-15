---
name: ferrite
description: "Drive other Ferrite Threads: start an agent, prompt it, wait for it, read its transcript. Use only when the user asks to delegate work to, coordinate with, or check on another Ferrite Thread or agent. Requires FERRITE_THREAD_ID and FERRITE_SOCKET, which Ferrite sets for every agent it runs."
---

# Ferrite

Ferrite runs coding agents (Claude, Codex) as **Threads**: durable
conversations, each served by a live agent Session, owned by a background
server that keeps running when no window is open. The `ferrite` CLI talks
to that server. Through it you can start other Threads, prompt them, wait
for them and read what they said.

## Check that you run inside Ferrite

```bash
test -n "${FERRITE_THREAD_ID:-}" && test -n "${FERRITE_SOCKET:-}"
```

If the check fails, say that you are not running inside Ferrite and stop.

Call the CLI through `FERRITE_BIN`: Ferrite's binary is usually not on
`PATH`.

```bash
F="${FERRITE_BIN:-ferrite}"
"$F" help
```

`"$F" help` and `"$F" <group>` (for example `"$F" agent`) print the
commands, their arguments and flags. Every command answers one JSON line
on stdout. A refusal is one JSON line on stderr, `{"id":…,"error":{"code":…,"message":…}}`,
with exit status 1; a wrong command line exits 2. Read ids and states from
the answers; never guess them.

## Threads and Attention

A Thread is named by its number, `thread`. `FERRITE_THREAD_ID` is your own;
`--current` stands for it wherever a command takes a Thread.

Every answer about a Thread carries its **attention**, one of:

- `working`: the agent, or only its subagents or background tasks, is at work.
- `blocked`: a **Decision** waits — a permission, a plan approval or a
  question that only the user can answer.
- `done`: it finished a turn the user has not looked at yet.
- `idle`: anything else, including a failed turn or an ended Session.

`done` and `idle` both mean the agent is ready for a prompt.

```bash
"$F" thread list
"$F" thread get 12
"$F" thread get --current
```

## Start a Thread and give it work

Start a Thread in a directory (yours when `--cwd` is left out). Use the
Provider the user asked for; leave `--provider` out to use their default.
Give it a short title so the user can tell it apart:

```bash
"$F" thread start --provider codex --title reviewer
```

The new Thread's number is `.result.thread.thread`. Add `--worktree` only
when the user wants the work kept apart on its own branch. To retitle a
Thread you started:

```bash
"$F" thread rename 14 "diff reviewer"
```

Prompt it, and wait for it to finish:

```bash
"$F" agent prompt 14 "Review the uncommitted diff and report only actionable findings." --wait --timeout-ms 900000
```

- `agent prompt` refuses with `agent_blocked` while a Decision waits on the
  Thread, and sends nothing.
- A prompt to a working agent waits its turn in the agent's queue. Until the
  agent has said it can queue (early in its Session), such a prompt is
  refused `failed` and nothing is sent: `agent wait` for it to settle, then
  send it.
- A prompt to a Thread whose agent is still starting is held until it is up.
  Only one is held: another is refused `agent_starting` and not sent; send
  it again once `thread get` shows the agent `working`.
- Put `--` before a prompt that starts with `-`:
  `"$F" agent prompt 14 -- "--help is broken"`.
- With `--wait` it answers only after the agent was seen working and then
  settled (`idle`, `done` or `blocked`); a turn that was already running
  when you sent it does not answer it. It fails `agent_prompt_stalled` when
  the agent shows no work within 5 seconds of its Session being up, and
  `timeout` when `--timeout-ms` runs out first. Without a timeout it waits
  as long as the work takes.
- A settled wait answers with the Thread's attention: `blocked` means it
  stopped at a Decision, not that it finished.

To wait on a Thread already at work, or for one particular state:

```bash
"$F" agent wait 14
"$F" agent wait 14 --until blocked --timeout-ms 600000
```

Without `--until` it waits until the agent settles, as `--wait` does.

Read what it said, as plain text (the last 80 lines unless `--lines`):

```bash
"$F" agent read 14 --lines 200
```

If the answer you need is longer than you can read this way, ask the agent
to write it to a file and reply with only the path; then read the file.

## Decisions

```bash
"$F" decision list
"$F" decision list --thread 14
```

Each entry names its `thread`, the Decision's `decision.id`, its kind and
what the agent wants to do. Only the user decides what an answer says; when
they have, pass it on:

```bash
"$F" decision answer 14 perm_01 --answer '{"kind":"deny","message":"Not on main."}'
```

The answer's shape follows the Decision's kind (`"$F" --schema` has them
all): `allow` with the tool's `input`, `deny` with a `message`, `choose`
with a `value`, `questions` with `answers`, `form` with `values`, `cancel`.
The first valid answer wins; a later one is refused `already_answered`.

## Follow what happens

```bash
"$F" events subscribe --threads 14
```

The first line is the state now (`threads`, `decisions`); every later line
is one event under the same id: `thread_opened`, `thread_closed`,
`attention`, `decision_requested`, `decision_closed`. It runs until you stop
it. Prefer `agent wait` when you only need to know when a Thread settles.

## Coordinate a project

A project's coordinator and its threads are Projects' (ADR 0030), not this skill's: run `ferrite-projects skill` and follow what it prints.

## Tell the user

```bash
"$F" notification show "Review ready" --body "3 findings in Thread 14"
"$F" thread focus 14
```

`notification show` puts a toast in every Ferrite window (`windows` says how
many saw it); `thread focus` brings a Thread on screen. Focus a Thread only
when the user asked to see it: it moves their view.

## Another machine

Only when the user asks for work on one of their saved machines (Ferrite's
remote computers), add `--machine <name>` to a command: it asks that
machine's server instead, over ssh.

```bash
"$F" thread list --machine build-box
"$F" agent prompt 3 "Run the test suite." --wait --machine build-box
```

A machine's Threads have its own numbers: name them by number, never with
`--current` (refused). Starting a Thread there needs `--cwd` with an
absolute path on that machine. `server_unavailable` from a machine says why: no
`ferrited` on it, or the user must sign in to it from a Ferrite window
first. Tell the user; do not retry in a loop.

## Safety rules

- **Never answer another agent's Decision without the user.** A Decision is
  the user's. Tell them what is waiting (`decision list`, `agent read`) and
  answer only with the answer they gave you.
- **A timeout or a stall does not prove the prompt was not delivered.** The
  agent may have it and be about to start. Read the Thread (`thread get`,
  `agent read`) before doing anything else.
- **Never resend a prompt blindly.** Resend only after reading the Thread
  shows the prompt never arrived, and never to a `blocked` Thread.
- Do not prompt your own Thread (`FERRITE_THREAD_ID`): a prompt to yourself
  waits behind the turn you are in.
- Start only the Threads the user asked for; each one is a live agent that
  costs them money. Do not rename, focus or notify about Threads you were not
  asked to touch.
- Reaching no server (`server_unavailable`) after a request was sent does not
  mean it was not carried out: check with `thread list` before trying again.
