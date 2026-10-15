---
status: accepted
date: 2026-10-15
---

# Coordinator Threads

Follows the Herdr benchmark (`docs/research/herdr-benchmark-2026-10-15.md`,
"Features taken from plugins" → Coordinator Threads), on the API of ADR
0027. The pattern is herdr-projects' (MIT): ideas only, no code.

## Context

Running a larger piece of work across agents means briefing each one,
keeping track of who does what, and carrying what one learned to the
next. herdr-projects does it with one coordinator agent that never does
the work itself: it proposes tasks, starts an agent per task on its own
branch with the same goal, instructions and memory, and Herdr's sidebar
shows which need the operator. Ferrite has the pieces — the API and the
skill, worktree Threads, Groups, the `gh` reading behind the Pane
header — but not the bookkeeping that ties them together.

## Decision

### One coordinator per Project, kept by the Server

A **Coordinator** is a Thread in its Project's main checkout, briefed by
its first prompt to plan with the operator and delegate everything. The
Server keeps the book in `coordinators.json` beside the Store, one entry
per Project: the coordinator, the Project's goal and standing
instructions, whether its PRs are followed, and its workers (each with its
task and the PR feedback already sent to it). The Project's **memory** is
a Markdown file of its own under `memory/` beside the Store, so the
operator can read and edit it; never inside a repo.

Starting a coordinator for a Project that has one answers that one
(reopened when parked): a Project has one goal and one memory, and two
coordinators would brief workers from the same memory at cross purposes.

### Driven through the API, so the coordinator agent drives it

Four operations join the API's table, append-only:

| Operation | Does |
|---|---|
| `coordinator.start` | the Project of `cwd`'s coordinator, started and briefed, or the one it has |
| `coordinator.get` | its brief, memory and workers, open ones in need order |
| `coordinator.update` | goal, standing instructions, PR follow-up |
| `worker.start` | a Thread in a new worktree of the Project, seated in the coordinator's Group, briefed with the Project's brief and its task |

`worker.start` refuses `not_a_coordinator` for a Thread that leads no
Project. A worker that has started is the coordinator's whatever fails
after (its Group seat, its title): the coordinator is told only of a
failure it could act on, so it never starts the same task twice.

The skill teaches the commands; the coordinator's brief tells it how to
work. The **go-ahead** is the coordinator's own Decision: it proposes the
tasks with its question tool (a Question the operator answers), or ends
its turn with the proposal where its Provider has none. The Server does
not gate `worker.start` on a synthetic Decision of its own: a Decision is
a Provider's request, and inventing a second kind for this one act would
be a new concept for little — the operator sees every worker start in
the nav, and the API is owner-only anyway.

### The Group, read by what needs the operator

A coordinator's first worker makes a Group with the coordinator first,
titled as the coordinator is; later workers join it. The Group remembers
its coordinator (`coordinator` in `groups.json`, optional, so older
windows read the file unchanged and older files load). The nav lists such a
Group's coordinator first and its workers by need — **needs you**
(blocked), **ready for review** (done or idle: a worker that finished,
seen or not, waits for review until it works again), **working** —
creation order among equals, whatever the sort, and holds still under the
pointer like every other ranked row (C8). Members are not reordered in
the file: the Group's Panes keep their places on the board.

A window starts a coordinator from the palette, `coordinate <project>`,
through a new gen-1 command `coordinate {project, provider}` answered
with the Thread, and lands on it. A window running its Engine in-process
(`--demo`, tests) is refused: only a Server coordinates.

### Lessons flow back through `## Remember`

A worker's brief asks it to end its report with a `## Remember` section,
one lesson per bullet. The Server watches each worker's Attention; when a
turn ends (working, then done or idle) it reads that turn's answer as the
API's `agent.read` does and appends the lessons the memory does not hold
yet. Later briefs carry the memory as it is then.

### PR feedback, opt-in

With `follow_prs` on, the Server reads each open worker's PR every minute
(the Pane header's pace for `gh`), off the pump thread: `gh pr view` with
the check rollup, the reviews and the conversation comments. Each failing
check (named by its run, so a rerun's failure is new) and each review or
comment with words in it is sent to the worker once, together, through
`agent.prompt`'s own rules: queued behind a working turn, never sent to a
blocked one. What could not be sent is tried at the next reading. A merged
or closed PR asks nothing. Line comments on the diff are not in `gh pr
view` and are not followed.

## Consequences

- Every act a coordinator takes is an API request: a script can do what
  a coordinator does, and the operator's palette does what a script can.
- Worktrees and branches the workers made are left in place: the plan
  rules worktree removal out.
- A turn shorter than one pump pass is not seen working, and its lessons
  are not read; turns take seconds, passes milliseconds.
- An unreadable `coordinators.json` is logged and started afresh, as
  unreadable settings are.
