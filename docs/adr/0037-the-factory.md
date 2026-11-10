---
status: accepted
date: 2026-11-10
---

# The Factory: Tasks through Workflows the operator authors

Spec: issue #199. The browser prototype it was settled on is
`spikes/software-factory-prototype-v2` on `prototype/software-factory`:
a throwaway, not code to port.

## Context

Ferrite has tried delegated work twice. Coordinator Threads (ADR 0028)
were removed for vendored herdr-projects (ADR 0030, 0031), and that board
was removed on 2026-11-03 because the operator did not use it. Both fixed
the process: a coordinator briefing workers on branches, then a board with
its own keeper, review and landing rules. `CONTEXT.md` still recorded
"issue-tracker/board integration: later, not v1".

What the operator wants is to hand off a fix or a feature and come back
only for questions and approvals, through a process they wrote: which
agents, in what order, with which instructions, skills, model and Effort,
who approves, where rework goes, what counts as finished. The pieces a
process needs already exist: the Server owns every Session (ADR 0025) and
its API starts, prompts, waits on and reads Threads (ADR 0027).

## Decision

A Project has a **Factory**: a Coordinator, Workflows and a board of
Tasks. It is Ferrite's own code, not vendored, and it prescribes no
process.

### One module the Server steps

The Factory is a headless module in core that knows nothing of the Server
or a Provider. Everything that changes it is one command through one door,
whoever asks: a window, the Coordinator, a Worker, a script. Once a pass
the Server gives it the time and what its Workers' Threads are doing, and
it answers with what to do: start a Thread, prompt one, park one, tell
the windows. The Server carries that out through `Engine::execute`, as
any API caller's act is. A change is written beside the Store before
anything that depends on it starts. Nothing of the removed
`server/projects.rs` returns.

Its operations join the API's one table (ADR 0027), so the CLI has them
with no second declaration.

### A Worker is a Thread its Task owns

Each Step of a Task runs in a Thread: a **Worker**. Ownership is recorded
on the Thread and survives a restart. An owned Thread is left out of the
nav, the palette's Thread search, the Inbox and the Attention list, and is
reached through its Task, where its Pane is the ordinary Pane. The API
still lists it, naming its owner. The Coordinator is owned by the Factory
the same way.

A Task returning to a Step it has visited continues that Step's Worker,
unless the Step is set to start fresh every time, as a reviewing Step
should be. A Task has one worktree, which its Workers share; it is removed
once the Task completes with a merged pull request and a clean checkout,
never forced.

### An outcome is reported, never read from prose

A Worker reports through the API: finished, unable to finish, a merge
result, or a question for the operator. Routes follow the reported
outcome. A Worker that is at rest (its turn ended, no Subagent or
background task running) without having reported is asked once to report;
if it does not, its Task needs the operator. A turn that ended while a
background task still runs is not at rest.

### Questions and approvals are a Task's states, not Decisions

A Decision is a Provider's request (ADR 0028 declined a second kind, and
that holds). A Worker's question, a result ready to approve and a stop are
states of the Task, with their own operations. A Provider's Decision in a
Worker's Session is still a Decision, answered in the Task. Neither
reaches the Inbox: the board's "needs you" is where the Factory asks. A
Task that needs the operator, is ready to approve, stops or completes is
told by toast, sound and system notification; a Task moving from one Step
to the next is silent.

### The Coordinator proposes

The Coordinator is an ordinary Provider Thread, briefed and given the
skill, so Ferrite still runs no agent loop of its own. It reads and drafts
freely. Starting a Task, saving a Workflow version, turning automatic
starts on and redirecting a Task are proposals the operator accepts in
the conversation.

### Starts

A Task starts from the operator or an accepted proposal, on a schedule, or
from a premade source: a GitHub issue or pull request, by label, by being
opened, or by a review request. Ferrite polls with the operator's `gh`
about once a minute, off the pump, as the Pane header's reads already do,
with conditional requests; nothing is installed on GitHub, and what
matched while the Server was down is found at the next check. One Task per
issue or pull request. Saving a Workflow starts nothing; automatic starts
are turned on separately, and turning one on over a backlog asks whether
to take what already matches. A setting bounds how many Tasks work at
once; the rest queue.

### The operator's own

Workflows are kept beside the Store, per operator. They are never files in
the repository: someone who clones it does not get them. Workers run with
the operator's Provider defaults.

## Consequences

- Reverses "issue-tracker/board integration: later, not v1". The
  operator's tracker stays the tracker: the Factory reads it to start
  Tasks, and writing to it is a Worker's own act with its own tools.
- Threads now have an owner, and four surfaces filter on it.
- Merging is a Step like any other. There is no model-free merge, no
  operation catalog for tools, and no built-in classifier of what is safe
  to merge.
- After a restart, a working Task's Worker is revived once and told it was
  interrupted. A second failure stops the Task; a Step is never run again
  from the start unasked, so nothing external is repeated blindly.
- A Factory belongs to the Server that owns its Project. A window on
  another Machine sees that Machine's.
