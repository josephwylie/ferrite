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
- A prompt to a Thread whose agent is still starting is held until it is
  up, and so is every prompt after it, in order.
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

## Tell the user

```bash
"$F" notification show "Review ready" --body "3 findings in Thread 14"
"$F" thread focus 14
"$F" project focus 3
```

`notification show` puts a toast in every Ferrite window (`windows` says how
many saw it); `thread focus` brings a Thread on screen, `project focus` a
Project's first open Thread. Focus only when the user asked to see it: it
moves their view.

## Stop, close and clean up

```bash
"$F" agent interrupt 14
"$F" thread close 14
"$F" worktree remove ~/src/.worktrees/app/my-task
```

`agent interrupt` stops the running turn, as the user's Esc does.
`thread close` parks a Thread: its agent stops, its history stays.
`"$F" thread revive 14 --resume <provider_session>` starts a parked
Thread's agent again in place, keeping its id (`--args`, `--env` and
`--launcher` as `thread start` takes them; without `--resume` it starts a
new conversation under the Thread's history).
`worktree remove` removes a worktree by its directory (a Thread's
`worktree`), parking the Threads in it; its branch stays. It is never
forced: a worktree with modified or untracked files is refused
`dirty_worktree_requires_force` and left whole. Close or remove only what
the user asked you to.

## Projects

A **Project** is a directory (usually a repository) the user's Threads
work in; every Thread names its `project` by id. `thread get` also says
whether the user is looking at the Thread (`focused`), whether they have
unsent text in its prompt box (`draft`: do not prompt it then), its
prompts `held_prompt` and `queued_prompts`, its `worktree` and `branch`,
its agent's own conversation id (`provider_session`), and
`attention_seq`, which grows with every change of attention (one count
for the Server's `boot`, which a restart changes). There are
no trust or permission screens to read: whatever the agent asks the user
is a Decision, and its Thread is `blocked` until it is answered.

```bash
"$F" project list
"$F" project get 3
"$F" project open --cwd ~/src/app --label app
"$F" project rename 3 billing
"$F" project close 3
"$F" worktree create --branch my-task --base main
```

`project open` answers the Project holding a directory, registering one
when there is none. `project close` parks every open Thread of it.
Start a Thread in a Project with `thread start --project 3` (it works in
`--cwd` itself, any directory inside the Project), on a new
branch with `--worktree --branch my-task --base main`, in a worktree that
stands (one `worktree create` made) with `--cwd` its path, or carrying on
an agent's own conversation with `--resume <provider_session>`. `--args`
are written as for the agent's terminal UI; Codex's `--model`, `-m`,
`-c model_reasoning_effort=…`, `-a`, `-s` and
`--dangerously-bypass-approvals-and-sandbox` become its app-server's
settings. Add
`--ready` to be answered only once its agent is ready for a prompt
(`agent_not_ready` names the Thread otherwise). To be told as soon as a
prompt was taken, rather than when the turn ends:

```bash
"$F" agent prompt 14 "Go on." --until working --until blocked --timeout-ms 8000
```

`"$F" machine list` names the saved machines `--machine` can ask, and
`"$F" notification show "Ready" --sound done` rings a chime with the
toast (`--important` keeps it up).

## The Factory

A Project's **Factory** runs **Tasks** the user hands off: each Task gets
its own worktree and branch, and a **Worker** (an agent Thread the Task
owns, `owner: {"task": "t-1"}`) does it. Start one only when the user asks.

```bash
"$F" task start "Fix the flaky login test" --project 3
"$F" task list --project 3
"$F" task get t-1
"$F" task stop t-1
"$F" task resume t-1
"$F" task delete t-1
"$F" task answer t-1 "Call the flag --fast"
"$F" task approve t-1
"$F" task rework t-1 "Add a test for the flag" --to step:s-3
"$F" task redirect t-1 s-3 --skip s-2 --then me --note "Only the docs changed"
"$F" task clear --project 3
```

A Task waits in Needs human for a question (`task answer`) and in Ready to
approve for approval (`task approve`, or `task rework` with feedback).
`task redirect` sends one Task to a Step, skipping Steps, then following
the routes or returning it to the user (`--then me`); the saved Workflow
is unchanged. Do these only when the user asks.

Completed Tasks leave the board for the Project's Archive 7 days after
they complete; `task clear` moves them now. Archived Tasks are kept whole
and still listed by `task list` (with `archived_ms`). A Worker interrupted
by a restart or a crash is revived once and told so; a second failure
stops its Task with the reason, for the user to resume or redirect.

`task start` uses the Project holding the CLI's directory when no
`--project` is given. `task delete` takes a Queued, Stopped or Completed
Task; its worktree is removed only when clean (otherwise it stays and the
answer says why), and its branch always stays.

A **Workflow** is the user's own process for a Project: ordered **Steps**,
each an agent with a name, instructions, skills, provider, model, effort,
and routes for "finished" and "unable" (`next`, `step:<id>`, `approval`,
`complete`, `human`, `coordinator`). Change one only when the user asks; saving makes
a numbered version and starts nothing, and a draft with problems is
refused with every problem listed. A Task started on a Workflow
(`task start … --workflow w-1`) keeps the version it started with.

```bash
"$F" workflow create --template issue_to_reviewed_pr --project 3
"$F" workflow list --project 3
"$F" workflow get w-1
"$F" workflow rename w-1 "Docs fixes"
"$F" step add w-1 --after s-2
"$F" step edit w-1 s-3 --name Reviewer --instructions "Review the PR" --skills code-review --on-unable step:s-2 --fresh
"$F" step move w-1 s-3 0
"$F" step delete w-1 s-3
"$F" workflow approval w-1 --by agent --step s-4 --instructions "Approve when the checks pass" --after complete --rework-to step:s-3
"$F" workflow save w-1
```

A route to `approval` waits in Ready to approve. `workflow approval` says
who decides (`human` or `agent`; an agent needs a deciding Step and
instructions), what follows approval (`complete` or `step:<id>`) and where
rework goes (`step:<id>` or `coordinator`).

A Workflow's Tasks can start without the user: on a schedule, from a
GitHub issue or pull request, or from the user's own script. The start is
part of the draft (`workflow start`, then save); automatic starts are a
separate switch that saving never turns on. Turn it on, pause or resume
it only when the user asks. A script submits Tasks with an identity of
its own: the same identity again returns the Task it already started.

```bash
"$F" workflow start w-1 '{"type":"github_issue","github":{"repository":"acme/app","trigger":"labelled","label":"ready-for-agent"}}'
"$F" workflow automation w-1 on
"$F" workflow backlog w-1 false
"$F" task submit w-1 JIRA-12 "Fix the CSV export"
```

If you are a Worker, report your Task's outcome when you are done. Your
outcome is read only from a report, never from your messages, and a
report is always about your own Task: it is known from the Thread you run
in, and no report can name another Worker. Report with Ferrite's own
tools, which need no permission: `report_outcome` (`outcome`, `summary`),
`report_question` (`text`), `report_revision` (`head`, `pr`) and
`report_evidence` (`text`). Each command below is the same report; run it
only if those tools are not offered to you:

```bash
"$F" report outcome finished "Fixed the race in the login fixture"
"$F" report outcome unable "Tried X and Y; blocked on Z"
"$F" report question "Should the flag default to on?"
```

A question holds your Task until the user answers; end your turn after
asking. When you decide an approval, report `approved` (what you
verified) or `rejected` (your reasons):

```bash
"$F" report outcome approved "All checks pass on the reviewed PR"
"$F" report outcome rejected "The changelog entry is missing"
```

A Task can carry a pull request (`task start … --pr owner/repo#12` or its
URL). Report its head each time you push, naming the PR the first time,
and keep what a check showed as evidence; both are tied to the PR's
current revision:

```bash
"$F" report revision 1111111aaaa --pr acme/widgets#12
"$F" report evidence "cargo test: 12 passed"
```

A Step that routes by merge result (`step edit … --by-merge-result
--on-conflict step:s-6`) reports one of six results, `merged` only once
the merge is confirmed: `merged`, `conflict`, `pending`, `denied`,
`head_changed` (the PR is not at the approved revision) or `unknown`.
Only `merged` may route to Completed; `--on-pending wait` keeps waiting.

```bash
"$F" report outcome merged "gh pr view shows MERGED at 2b3c4d5"
"$F" report outcome unknown "gh pr merge timed out"
```

### The Coordinator

Each Project's Factory has one **Coordinator** (an agent Thread the
Factory owns, `owner: {"factory": 3}`, in the main checkout) that the
user talks to in the Factory's Coordinator view; it starts on their
first word. `coordinator say` speaks to it from a script, and
`coordinator set` chooses its provider, model and effort from its next
turn.

```bash
"$F" coordinator say "What is left on the board?" --project 3
"$F" coordinator set codex --model gpt-5-codex --effort high --project 3
```

If you are the Coordinator, read and draft freely (`task list`, `task get`,
`workflow get`, `step edit`, …). Your reads are your own tools too, which
need no permission: `task_list`, `task_get`, `workflow_list`,
`workflow_get` and `proposal_list`. But never act: starting a Task, saving a
Workflow version, turning automatic starts on and redirecting a Task are
**proposals**, shown to the user as a card they accept or dismiss. Nothing
happens until they accept. A one-off handoff needs no Workflow. Raise one
with your own `proposal_raise` tool (its `act` and the flags below as its
fields), which needs no permission; the command is for when that tool is
not offered. The Server refuses every other act from your Thread,
starting a Thread, prompting, stopping or parking one, and answering a
Decision included.

```bash
"$F" proposal raise start --instructions "Fix the toolbar overflow" --workflow w-1
"$F" proposal raise save --workflow w-1
"$F" proposal raise automation --workflow w-1
"$F" proposal raise redirect --task t-1 --to s-3 --skip s-2 --then me --note "Only the docs changed"
"$F" proposal list --project 3
```

`proposal accept` and `proposal dismiss` are the user's: run them only on
their word.

```bash
"$F" proposal accept p-1
"$F" proposal dismiss p-1
```

## Drive a headless window

To check a change to Ferrite's own UI, run a `ferrite --headless` built
with `--features headless` on a throwaway Store (`FERRITE_STORE`): it is
the real window, attached to that Store's server and drawn off-screen, so
it never covers or interrupts the user's. The `window` commands drive it:

```bash
"$F" window key cmd-,
"$F" window type "hello"
"$F" window find settings-close
"$F" window click --selector settings-close
"$F" window shot /tmp/settings.png
```

`window key` presses each keystroke in order (`unhandled` lists those
nothing took). `window click` takes an element's debug selector (its
`window find` bounds are where it was last drawn) or `--x` and `--y` in
the window's 1440x900 logical pixels, with `--double` or `--right`.
`window shot` writes a PNG (2880x1800) the window drew; look at it.
With no headless window attached the server refuses with
`no_headless_window`.

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
