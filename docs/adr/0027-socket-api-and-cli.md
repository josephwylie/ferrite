---
status: accepted
date: 2026-10-15
---

# The Server's API, the `ferrite` CLI and the agent skill

Follows the Herdr benchmark (`docs/research/herdr-benchmark-2026-10-15.md`,
§2 "Socket API and `ferrite` CLI" and §3 "Bundled agent skill"), on the
Server of ADR 0025.

## Context

With the Server owning every Session, an agent in one Thread could drive the
others — start a reviewer, prompt it, wait for it, read what it said — if
something outside a window could talk to the Server. Herdr does this with a
newline-delimited JSON API, a CLI over it, caller context in every pane's
environment and a bundled skill; the community's coordinator plugins
(herdr-projects) are built on nothing else. The plan takes Herdr's shape, not
MCP.

## Decision

### A socket of its own

The API listens on `api.sock` beside `server.sock` (on Windows the pipe
`link` derives from that path), with the same owner-only rules. ADR 0025
expected it to share the windows' listener with another `role` in `hello`;
a second socket is simpler and keeps the windows' frozen contract untouched:
the API has its own envelope and its own stability rules, a CLI never says
`hello`, and nothing in `link`'s outbox (lanes, coalescing, the lag limit,
history) applies to one request and one answer. Both feed the pump's one
channel (`link::Heard::Api`), so a request is answered on the pump thread
between passes, in order with the windows' commands. If `api.sock` cannot be
bound the Server runs without it, and its agents are not told a socket.

### The wire

Herdr's shape: one JSON object per line.

- request `{"id":"1","method":"agent.prompt","params":{…}}`
- answer `{"id":"1","result":{…}}` or `{"id":"1","error":{"code","message"}}`
- after `events.subscribe` succeeds, `{"id":"1","event":{"event":…}}` lines.

A connection may ask several things; a wait holds its answer, not the
connection. `id` is a string; a request whose `id` is not one is refused
`invalid_request` with that id's JSON text, so `7` comes back as `"7"`.
A connection whose reading side closes (`echo … | nc -U api.sock`
half-closes once it has asked) is still answered: it is let go once no wait
or subscription owes it a line. A connection that is gone is found when a
line cannot be written to it, and its waits and subscriptions end then; a
wait from a caller killed mid-wait lingers until it settles. API connections are numbered from
2³², so a Decision's `answered {client}` names an API caller apart from any
window.

### Each operation declared once

`api::schema` declares every operation in one `operations!` table, after
Herdr's `define_operations!`: wire name (`group.verb`), parameter type, the
parameters the CLI takes positionally, and a summary. From it come the wire
enum (`Method`, adjacently tagged by `method`/`params`), the catalog the CLI
and `ferrite help` read, and the JSON Schema (`schemars`, already in the
tree through gpui-component; `ferrite --schema`). The CLI has no second
declaration: flags are the parameters' names in kebab case, typed by their
schema (a boolean is a bare flag, a list repeats, anything structured is
JSON). Names are append-only and parameters added later are optional, as in
the client contract; each operation's request is frozen as a fixture under
`api/fixtures/v1`.

One exception: ADR 0028's `coordinator.start`, `coordinator.get`,
`coordinator.update` and `worker.start` were removed with the design they
served when ADR 0030 made Projects Ferrite's coordinator, fixtures and all.
A caller that still asks for one is refused as for any unknown method.
So was `nav.focus`, with the window's `nav_focus` message, when ADR 0031
removed the 0.2 Projects that alone called it: an older Server's
`nav_focus` is an unknown message type to a newer window, which ignores
it (ADR 0025).

| Operation | Does |
|---|---|
| `thread.start` | `open` a Thread in `cwd` (a fresh worktree with `worktree`), titled |
| `thread.list` / `thread.get` | open Threads with their Attention |
| `thread.focus` | `focus {thread}` to every window, which lands on it as a nav row does |
| `thread.rename` | the window's `rename` |
| `agent.prompt` | refuse `agent_blocked` while a Decision waits; else `send`, or `queue` when the Composer would; `wait` as below. A Session still starting gets `send`, which holds one prompt until it is up |
| `agent.wait` | until settled, or `until` one Attention |
| `agent.read` | the last lines of the transcript as plain text (`transcript::plain_text`, the window's copy) |
| `decision.list` / `decision.answer` | pending Decisions still unanswered (one whose answer is being submitted, as an async question's is until its provider closes it, is not listed); an answer named by the provider's id, through the window's `answer` (first valid wins) |
| `events.subscribe` | snapshot, then `thread_opened`, `thread_closed`, `attention`, `decision_requested`, `decision_closed` |
| `notification.show` | `notification {title, body}` to every window, drawn as a toast |

Added for Projects (ADR 0030), each what herdr-projects asks of Herdr:

| Operation | Does |
|---|---|
| `thread.start` | gains `focus`, `args` (the CLI's, written as for its terminal UI as Herdr launches it, after Ferrite's own, for every Session of that provider; saved beside the Thread's log, so a revive or a Server restart keeps them; Codex's model, `model_reasoning_effort`, approval, sandbox and `--dangerously-bypass-approvals-and-sandbox` flags become `codex app-server`'s thread settings, which only the Thread's own model or effort choice overrides, and the rest pass to it unchanged), `resume` (the provider's own conversation id to carry on), `branch`/`base` (a new worktree on that branch; a `cwd` that is a linked worktree already is opened as one), `project` (`cwd` must lie in it; the Thread works in `cwd` itself, as Herdr's tab does, and a fresh worktree is of the Project directory holding it), and `ready` with `timeout_ms` (answered once the agent is up and idle or done, else `agent_not_ready` naming the Thread; 20 s by default) |
| `thread.close` | the window's `park` |
| `agent.prompt` | gains `until`: answered as soon as the agent is seen in one of those states after the prompt (`working`, `blocked`: the prompt was taken) |
| `agent.interrupt` | the window's `interrupt` |
| `project.list` / `project.get` | the registry's Projects: `id` (its number, as a string), `label` (its title), `focused`, `thread_count` (open), `worktree` (`repo_key`, `checkout_path`, `is_linked_worktree`, read from `.git`) |
| `project.open` | the Project holding `cwd` (or a linked worktree's repository), else `cwd` registered, titled `label` |
| `project.rename` / `project.close` | the window's project rename; park every open Thread of it |
| `worktree.create` | `git worktree add -b branch` from `base` in the registry's layout, for a Thread to start in; `{worktree {path, branch}, project}` |
| `worktree.remove` | by `path`: `worktree_not_found` unless a linked worktree is there; refuse `dirty_worktree_requires_force` unless `git status` is clean, then park its open Threads, `git worktree remove` (never forced), bind its Threads to the main checkout; the branch stays |
| `machine.list` | the enabled saved machines, `{id, label, target}` by name |

`notification.show` gains `sound` (`request`, `done`, `none`) and
`important`. A Thread's answer gains `focused` and `draft`, which the
windows tell the Server (`focused`, `composing`, gen-1 commands only a
Server hears), `project`, `provider_session` (the provider's own id,
which it resumes from), `attention_seq` (the change its Attention was
last counted at, one count for every Thread of the Server's `boot`, so
a revived Thread counts on past its park; Thread ids hold across a
restart, ADR 0029, the count and `boot` do not, ADR 0031), `held_prompt`, `queued_prompts`, `worktree` and its `branch`.
Herdr-projects reads a Terminal's screen for a draft in the prompt box
and for trust dialogs; Ferrite answers the first with `draft` and has no
second: whatever an agent asks the operator is a Decision, so `blocked`. `ferrite
--version` prints `ferrite <semver>`. No open Thread stays
`unknown_thread` (Herdr's `pane_not_found`; codes are append-only like
names), beside the new `project_not_found`, `worktree_not_found`,
`dirty_worktree_requires_force` and `agent_not_ready`; the codes are
listed in `api::schema`.

Every act that changes a Thread is the window's own command through
`Engine::execute`, so an agent and the operator go through one door. `focus`
and `notification` are new gen-1 server messages; a window too old for them
ignores them, as the contract allows.

### Attention is the API's state

The API reports and waits on **Attention** (ADR 0015), computed by the
Server from what it knows exactly (`Engine::attention`): blocked while a
Decision waits, working while Main or its background work runs, done while
an unseen Notice stands, else idle. A prompt not yet taken counts as work:
one in the provider's queue (or being taken into it), since the turn before
it can end a pass before it starts, and one held for a starting Session (a
held send or a new Thread's first prompt), so a plain `agent.wait` after
`agent.prompt` is not answered by the idle before the agent has the prompt.
`Attention` moves to core; the window reads the same type.

### Waiting, Herdr's semantics

- `agent.prompt` refuses `agent_blocked` before sending anything.
- A prompt to a Session still starting (a Server's staggered revival, a
  model or provider change) is held, as a send to a stopped Session is; the
  Thread holds one, so another is refused `agent_starting` rather than
  replace it. A prompt to a working agent goes to the provider's native
  queue, as the Composer's does; until the provider has confirmed it can
  queue, that is refused `failed` and nothing is sent (the window's Composer
  keeps its draft in the same case).
- With `wait`, the answer comes only after the agent was seen working (or
  blocked) and then settled: idle, done or blocked. The idle it was sent
  from never answers it. A prompt delivered marks its Thread working at
  once, so the wait looks at the Thread right after sending as well as on
  every pass: a turn shorter than one pass is not missed.
- No work seen within five seconds answers `agent_prompt_stalled`. Herdr
  counts from submission; Ferrite counts from the first moment the
  Thread's Session is up, because a prompt to a Session still starting is
  held until it is, and a cold start can take longer than five seconds.
- `timeout_ms` counts from the request and answers `timeout`. Neither a
  stall nor a timeout undoes anything or proves the prompt was not
  delivered; the messages say so.
- `agent.wait` has no prompt to see land: settled by default, or one
  Attention with `until`.

The rule is a pure state machine (`api::Wait`), tested without a Server.

### Subscribe before the snapshot

`events.subscribe` registers the subscription, then answers with the
Threads and Decisions as they are, all on the pump thread: no change after
the snapshot is missed, and one may be told twice. Decision events are the
windows' own `decision`/`decision_closed`; the rest are differences the API
takes after each pass.

### Caller context

Every Session's environment carries `FERRITE_THREAD_ID`, and, when the
Server takes API connections, `FERRITE_SOCKET` and `FERRITE_BIN`. The plan
says every Session; only a Server has an API, so a Session run in-process
(`--demo`, tests) or under a Server whose `api.sock` could not be bound gets
the Thread id alone. `FERRITE_BIN` is the Server's executable: an app
bundle's binary is rarely on `PATH`. They are set over whatever the CLI
inherited, and stripped from a Session's environment where Ferrite has no
API, like the other inherited markers. `--current` reads `FERRITE_THREAD_ID`.

### The CLI is the app's binary

`ferrite <group> <verb>` is dispatched in `main` before anything touches
GPUI, as `ferrite server` is; the parser lives in core (`api::cli`). After
`--` every word is an argument, so a prompt may start with dashes. It
prints the Server's answer line as it came; a refusal goes to stderr with
exit status 1, a wrong command line exits 2 (Herdr's convention). It does
not start a Server: agents run inside one, and a script that finds none is
told `server_unavailable`.

### The skill

`skills/ferrite/SKILL.md`, compiled into the binary: `ferrite --skill`
prints it, `ferrite --skill install` writes it to `~/.claude/skills/ferrite`
and `$CODEX_HOME/skills/ferrite`. It teaches starting, prompting, waiting
and reading through `$FERRITE_BIN`, and its rules: never answer another
agent's Decision without the user; a timeout or stall does not prove
non-delivery; never resend blindly; don't prompt your own Thread.

## Consequences

- Any process of the operator's can drive their agents, as any window can.
  The socket is owner-only; nothing else guards it, as in Herdr.
- `agent.read` is the transcript as the window copies it, not the provider's
  raw stream; thinking is left out.
- On Windows a release build is a GUI-subsystem executable with no console,
  so the CLI's output is not seen from a terminal there yet.
- `--machine <name>` (plan §2) asks a saved machine's Server (ADR 0026),
  found by name, else by ssh target; one turned off is refused. The CLI
  runs `ssh -T … <target> ferrited api-bridge` with the window's options
  (`machine::bridge_args`, one builder for both): `BatchMode=yes`, so it
  never prompts, and the window's master under `~/.ferrite/machines/`, so
  a window's sign-in carries it. `ferrited api-bridge` carries stdio to
  the machine's `api.sock` as `bridge` does to `server.sock`, but starts
  no Server: with none, its words come back as `server_unavailable`. It
  is a word of its own, not a flag on `bridge`, because a `ferrited` older
  than it would read `bridge --api` as `bridge`, start a Server and speak
  the window protocol to the CLI; an older one refuses `api-bridge` with
  its usage, which the CLI reads as "update `ferrited` there". An ssh
  that broke with no answer is refused in the words of its `Failure` (no
  `ferrited`, or a sign-in the operator must do from a window); a CLI
  cannot sign in. Thread ids are per Store, so `--current` is refused with
  `--machine`, and `FERRITE_SOCKET` is not consulted. A set
  `FERRITE_THREAD_ID` is not refused: only `--current` reads it, and
  refusing it would keep every agent inside a Thread from `--machine`. Paths are the
  machine's too: a `cwd` is never filled in from this computer's
  directory, so `thread start` needs `--cwd` with an absolute path on
  the machine.
