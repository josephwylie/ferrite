---
status: accepted
date: 2026-11-22
---

# A Thread delegates work to another machine

T3 Code lets a thread's agent start a thread in a linked environment to do
one piece of work. T3's server follows that thread there and wakes the
agent with its result when it ends. Ferrite does the same with its saved
Machines, in T3's words and by T3's rules (ADR 0038). We call the Thread
started there a **Delegate**. This ADR covers its record, how it is
followed, how its result wakes the agent, how it is stopped, and where
Ferrite differs from T3.
The peer side is in ADR 0027: an idempotent `thread.start` with `prompt`,
`mode` and `delegated_from`, plus `thread.stop`, `agent.wait --turn-ended`
and `agent.result`. Peers is the Server's client of a saved machine.

## Decision

- **The parent's Server owns it.** The record, the follower and the wake
  all live on the Server that runs the parent Thread (ADR 0025), so they
  outlive windows and restarts. The Server pulls from the peer through
  Peers, as T3's linking side pulls. It never pushes back to a machine
  that may be asleep.
- **The record is a header fact on the parent.** `Delegates` (`delegate`
  in core) holds every Delegate the Thread started, oldest first, under
  ids `d1`, `d2` that are never given twice. It is amended through the
  Store as `pr_watch` is, open or parked, and saved before anything acts
  on it.
  - Each Delegate names its peer by `ServerId` plus its `ThreadId` there
    (G-A3), never by a saved name or target, which the operator may edit.
  - Each Delegate also carries:
    - the launch key `delegate:<parent ServerId>:<parent thread>:<key>`
      (G-A9);
    - its title and task, and the provider and model it runs there;
    - T3's status: `starting`, `running`, `waiting`, `completed`,
      `failed`, `cancelled` or `interrupted`;
    - its result, and the `problem` holding it up;
    - `stop_owed`;
    - `told`.
  - `Store::fork` drops the record (G-A10). A fork's agent did not start
    those Delegates.
- **One launch.** `delegate.start` sends one `thread.start` to the peer.
  It carries:
  - the task as its `prompt` (G-A5);
  - the key as its `request`;
  - the parent's reach as its `mode` (G-A6): what its provider announced,
    else this machine's default, and never more than the parent's own
    cap. A narrower `mode` may be asked; a broader one is refused
    `widens_cap` in T3's words ("Child runtime mode … is broader than
    parent mode …");
  - the model and effort as the CLI's own arguments (`--model`/`--effort`,
    `-m`/`-c model_reasoning_effort=`). As T3 does, a Delegate on its
    parent's provider inherits the parent's model, and on that model its
    effort; another provider inherits neither and must name its model;
  - `delegated_from`.

  Where it works there, when the start names no `checkout`, is T3's
  linked-environment project: the Project there named by `project`, else
  the one Project there whose repository is the parent's. A repository
  is its primary remote as `host/owner/name` (`workspace::repository_key`,
  T3's `normalizeGitRemoteUrl` and `pickPrimaryRemote`), so the two
  machines may spell the remote differently. `project.list` says each
  Project's `directory` and `repository`. None, several, or a parent with
  no repository is refused in T3's words before anything starts there.

  A repeated key answers with the first Thread, parked or not. The
  Delegate is recorded once the peer answers, and a record that fails
  stops that Thread there. This machine's Labs gate must be on, and so
  must the peer's.
- **One follower per open Delegate**, on a thread of its own (T3's
  `follow`):
  - It looks at the Thread there (`thread.get`), so the record shows
    `running` or `waiting` on a Decision. Then it waits on
    `agent.wait --turn-ended` in pieces of at most 50 s, and looks again
    between pieces. A blocked Thread is looked at once a piece, never in
    a loop (G-A2).
  - When the turn ends, or the Thread there is parked, it reads
    `agent.result`: the final reply in full, or a failed turn's error
    (G-A4, G-A8). The turn's outcome is the status.
  - A peer that does not answer is asked again, backing off from 2 s to
    5 min, and the `problem` says why. That covers a sleeping machine, a
    needed sign-in, or a missing or old `ferrited`.
  - Some answers are final, and fail the Delegate in their own words:
    - the Thread is gone there;
    - the machine was removed or turned off here;
    - another Server answers (Peers' `Final` and `OtherServer`).
  - Three unreadable answers in a row fail it with T3's give-up words.
  - A restarted Server starts a follower for each open record. It reads
    parked logs once, off the pump, for stops they still owe.
- **The wake is the Server wake (ADR 0046), generalised.** A Thread holds
  one waiting wake at most: a PR's news or its Delegates' endings. A
  Delegate wake is worded when sent. It covers every Delegate that ended
  untold since, in T3's `delegatedCompletionWakeDetail` words:
  - each Delegate's label: `Delegated task d1 (title) [status]`;
  - results in full while they fit 16 000 bytes between them;
  - `task_status` for the rest.

  Once the agent takes the wake, those Delegates are marked told.
- **Its parent waits on it.** A Delegate that is open, or ended and not
  told, holds its parent:
  - the parent's Attention is working;
  - `ended_turn` stays open, so a grandparent's follower waits for the
    whole tree (T3's `untilDelegatedWorkSettled`);
  - the parent's Notice defers.
- **Cancel, and stopping with the parent.**
  - `delegate.cancel` ends an open Delegate `cancelled` at once, with no
    wake, and owes a stop there (T3's `task_cancel`).
  - `thread.stop` on the parent, or parking it, ends each open Delegate
    `interrupted` with "Stopped with its parent.". It owes a stop for
    every Delegate, ended or not, since a Thread there may have run on
    (T3's `stopDelegatedTasks`, T3's archive).
  - A stopper sends `thread.stop` there until one gets there, across
    restarts, parked parents included. Until then a read of the result
    says "It could not be stopped on pc and may still run there: …".
  - A stop the peer refuses, or a Thread gone there, needs no stop.
  - Nested Delegates stop depth first: a Delegate's own `thread.stop`
    there stops its Delegates.
- **The API** is `delegate.start`, `delegate.get`, `delegate.list` and
  `delegate.cancel` (ADR 0027), with the CLI's `ferrite delegate …`.
  An ended Delegate that its parent's agent reads is told: no wake carries
  it again (T3's `task_status`).
- **The agent tools are T3's.** Every Thread's agent gets
  `orchestrator_capabilities`, `delegate_task`, `task_status`,
  `task_cancel` and `machine_list` (T3's `t3_environment_links`), with
  T3's descriptions, parameters and answer shapes. They are Ferrite's own
  pre-allowed tools (`api::tools`, the mechanism PR watch uses): each call
  is one or two API requests from the Session's Thread, translated
  between T3's names and the API's in `api/tools/delegation`.
  - They are offered while this machine's Settings › Labs › Linked
    machines is on when the Session starts, as T3 hides them while links
    are off. The gate is the machine's own (`ServerDefaults`, T3's
    `serverScoped`), so Settings on a machine sets that machine's.
  - A subagent's call acts for its Thread, as T3's does.
  - `delegate_task` answers T3's task with its `next` (do not poll; end
    the turn). `machine_list` asks every machine at once and gives each
    3 s (T3's list probe): `reachable`, `unreachable` or `sign_in`.
    `orchestrator_capabilities` with a `machineId` is that machine's own
    `provider.list`.
  - Refusals are the Server's words, T3's where T3 has them: Linked
    machines off here or there, a machine not linked, one that did not
    answer, one that needs a sign-in.

## Operator decisions

- **Interrupting the parent's Main does not cancel its Delegates.** This
  overrules T3, whose Stop stops delegated tasks. It keeps Ferrite's Main
  rule (`CONTEXT.md`): an interrupt stops Main's turn alone. `thread.stop`
  is the whole stop and does cancel them.
- **Parking the parent cancels its Delegates, and nesting is allowed.**
  Both follow T3: parking is T3's archive, and nested Delegates stop depth
  first.

## Known limit

Over ssh the peer cannot tell a Server's call from the user's, so the cap
binds the delegation path only (G-A6). `ferrite --machine pc thread start`
from inside a Thread reaches the same API as that user, and can start an
uncapped Thread there. Peers sends no `caller` (G-A11), and the peer
gives the parent no way to approve the Delegate's permission Decisions.
Nothing here fences the link itself, and T3 does not either.

## Where Ferrite differs from T3, and why

- **Saved first, then acted on.** T3 tries the stop before it records a
  cancel. Ferrite records the cancel and owes the stop, so a restart
  cannot lose it. The "could not be stopped" words show once the first
  stop fails.
- **A delivered wake is durable.** T3 queues the wake message in its
  store. Ferrite marks the Delegates told after the agent takes the wake.
  A restart between the two sends the wake again, but never loses it. A
  wake the Thread cannot take, because its Session will not start, is
  marked told: the result stays readable, and the refusal shows on the
  Thread.
- **`waiting` is shown.** T3's follower leaves a remote task `running`
  until it ends. Ferrite looks between pieces, so a Delegate that waits
  on a Decision there reads `waiting`. No status is `queued`: a Delegate
  is recorded once its Thread there exists.
- **A machine, not an environment.** "On pc" and "the linked machine"
  stand for T3's "in {label}" and "the linked environment".
- **The tools take only what Ferrite carries out.** A parameter Ferrite
  cannot honour is not in the schema, never taken and dropped:
  - `isolation` (a fresh worktree there), `mode: wait` and `timeoutMs`,
    `interactionMode`, `driverKind` and `search`;
  - a delegated task always names `target.machineId`: a child on this
    machine is a Thread of its own, not a Delegate;
  - the model and effort are not checked against the machine's catalog
    before the start; its CLI refuses a model it does not know.
- **A repository key is read, not asked.** T3 resolves ssh aliases
  (`ssh -G`) and `insteadOf` rewrites; Ferrite reads git's config file
  without running git, so a remote spelled through one keys as written.
- **No re-stop for a stop already under way.** A stop owed again while
  one is out is covered by that one (T3 looks once more).
